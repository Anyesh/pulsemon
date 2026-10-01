use std::collections::HashSet;
use std::fs;
use std::io;
use std::time::Instant;

use super::{Account, Field, Missing, ProcessDetail};
use crate::collectors::ports::linux::{parse_proc_net, pid_socket_inodes, TABLES};
use crate::collectors::users::UserNames;
use crate::types::PortInfo;

/// Replaces sysinfo's values with direct /proc reads, which tell a permission
/// failure apart from a field that does not exist (kernel threads have no exe).
pub fn read_common(pid: u32, detail: &mut ProcessDetail) {
    let path = |leaf: &str| format!("/proc/{pid}/{leaf}");
    detail.exe = read_link(&path("exe"));
    detail.cwd = read_link(&path("cwd"));
    detail.root = read_link(&path("root"));
    detail.argv = read_nul_list(&path("cmdline"));
    detail.environ = read_nul_list(&path("environ"));
    detail.ports = owned_ports(pid);
}

pub fn missing(err: io::Error) -> Missing {
    match err.kind() {
        io::ErrorKind::PermissionDenied => Missing::Denied,
        _ => Missing::Unavailable,
    }
}

fn read_link(path: &str) -> Field<String> {
    fs::read_link(path)
        .map(|p| p.display().to_string())
        .map_err(missing)
}

fn read_nul_list(path: &str) -> Field<Vec<String>> {
    let bytes = fs::read(path).map_err(missing)?;
    let items: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect();
    if items.is_empty() {
        Err(Missing::Unavailable)
    } else {
        Ok(items)
    }
}

/// Reads only this process's fds and the socket tables of its own network
/// namespace (`/proc/<pid>/net`), so a containerised process shows the sockets it
/// really holds rather than whatever shares an inode number on the host.
fn owned_ports(pid: u32) -> Field<Vec<PortInfo>> {
    let inodes: HashSet<u64> = pid_socket_inodes(pid)
        .map_err(missing)?
        .into_iter()
        .collect();
    if inodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut ports = Vec::new();
    for (file, protocol) in TABLES {
        let Ok(content) = fs::read_to_string(format!("/proc/{pid}/net/{file}")) else {
            continue;
        };
        ports.extend(
            parse_proc_net(&content, protocol)
                .into_iter()
                .filter(|(_, inode)| inodes.contains(inode))
                .map(|(mut port, _)| {
                    port.pid = Some(pid);
                    port
                }),
        );
    }
    Ok(ports)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub runtime: &'static str,
    pub id: String,
    /// Kubernetes pod uid, when the container runs under kubepods.
    pub pod: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cgroup {
    pub path: String,
    /// Deepest systemd unit (`.service` or `.scope`) in the path.
    pub unit: Option<String>,
    pub slice: Option<String>,
    pub container: Option<Container>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capabilities {
    None,
    Full,
    Some(Vec<&'static str>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limit {
    pub soft: String,
    pub hard: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub threads: Option<u32>,
    pub cap_eff: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stat {
    pub tty_nr: u32,
    pub nice: i32,
}

#[derive(Debug, Clone)]
pub struct Extras {
    pub cgroup: Field<Cgroup>,
    /// `Ok(None)` when no login session owns the process (daemons, kernel threads).
    pub login_user: Field<Option<Account>>,
    /// Namespace kinds this process does not share with pulsemon.
    pub own_namespaces: Field<Vec<&'static str>>,
    pub capabilities: Field<Capabilities>,
    pub threads: Field<u32>,
    pub fd_count: Field<usize>,
    pub open_files: Field<Limit>,
    pub nice: Field<i32>,
    pub tty: Field<Option<String>>,
    pub oom_score: Field<i32>,
    pub oom_score_adj: Field<i32>,
    pub pss: Field<u64>,
    pub swap: Field<u64>,
    pub collected: Instant,
}

const NAMESPACES: [&str; 7] = ["cgroup", "ipc", "mnt", "net", "pid", "user", "uts"];

/// Capability names indexed by bit, as in linux/capability.h.
const CAPABILITIES: [&str; 41] = [
    "chown",
    "dac_override",
    "dac_read_search",
    "fowner",
    "fsetid",
    "kill",
    "setgid",
    "setuid",
    "setpcap",
    "linux_immutable",
    "net_bind_service",
    "net_broadcast",
    "net_admin",
    "net_raw",
    "ipc_lock",
    "ipc_owner",
    "sys_module",
    "sys_rawio",
    "sys_chroot",
    "sys_ptrace",
    "sys_pacct",
    "sys_admin",
    "sys_boot",
    "sys_nice",
    "sys_resource",
    "sys_time",
    "sys_tty_config",
    "mknod",
    "lease",
    "audit_write",
    "audit_control",
    "setfcap",
    "mac_override",
    "mac_admin",
    "syslog",
    "wake_alarm",
    "block_suspend",
    "audit_read",
    "perfmon",
    "bpf",
    "checkpoint_restore",
];

pub fn read_extras(pid: u32, users: &mut UserNames) -> Extras {
    let path = |leaf: &str| format!("/proc/{pid}/{leaf}");
    let read = |leaf: &str| fs::read_to_string(path(leaf)).map_err(missing);
    let status = read("status").map(|s| parse_status(&s));
    let stat = read("stat").and_then(|s| parse_stat(&s).ok_or(Missing::Unavailable));
    let smaps = read("smaps_rollup").map(|s| parse_smaps_rollup(&s));
    let now = Instant::now();

    Extras {
        cgroup: read("cgroup").map(|s| parse_cgroup(&s)),
        login_user: read("loginuid").map(|s| {
            parse_loginuid(&s).map(|uid| Account {
                name: users.resolve_raw(uid, now).to_string(),
                id: uid.to_string(),
            })
        }),
        own_namespaces: own_namespaces(pid),
        capabilities: status
            .clone()
            .and_then(|s| s.cap_eff.ok_or(Missing::Unavailable))
            .map(decode_capabilities),
        threads: status.and_then(|s| s.threads.ok_or(Missing::Unavailable)),
        fd_count: fs::read_dir(path("fd")).map(|d| d.count()).map_err(missing),
        open_files: read("limits")
            .and_then(|s| parse_limit(&s, "Max open files").ok_or(Missing::Unavailable)),
        nice: stat.clone().map(|s| s.nice),
        tty: stat.map(|s| tty_name(s.tty_nr)),
        oom_score: read("oom_score")
            .and_then(|s| s.trim().parse().map_err(|_| Missing::Unavailable)),
        oom_score_adj: read("oom_score_adj")
            .and_then(|s| s.trim().parse().map_err(|_| Missing::Unavailable)),
        pss: smaps.and_then(|m| m.0.ok_or(Missing::Unavailable)),
        swap: smaps.and_then(|m| m.1.ok_or(Missing::Unavailable)),
        collected: now,
    }
}

/// Compares against our own namespaces rather than pid 1's, because reading
/// /proc/1/ns needs root.
fn own_namespaces(pid: u32) -> Field<Vec<&'static str>> {
    let mut differ = Vec::new();
    for ns in NAMESPACES {
        let theirs = fs::read_link(format!("/proc/{pid}/ns/{ns}")).map_err(missing)?;
        let ours = fs::read_link(format!("/proc/self/ns/{ns}")).map_err(missing)?;
        if theirs != ours {
            differ.push(ns);
        }
    }
    Ok(differ)
}

/// Reads cgroup v2 (`0::path`) or, on v1, the systemd hierarchy, which is where
/// units and container scopes are named.
pub fn parse_cgroup(content: &str) -> Cgroup {
    let entries: Vec<(&str, &str)> = content
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, ':');
            fields.next()?;
            Some((fields.next()?, fields.next()?))
        })
        .collect();
    let path = entries
        .iter()
        .find(|(controllers, _)| controllers.is_empty())
        .or_else(|| entries.iter().find(|(c, _)| *c == "name=systemd"))
        .or(entries.first())
        .map_or("", |(_, path)| path);

    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let deepest =
        |pred: fn(&str) -> bool| parts.iter().rev().find(|p| pred(p)).map(|p| p.to_string());
    Cgroup {
        path: path.to_string(),
        unit: deepest(|p| p.ends_with(".service") || p.ends_with(".scope")),
        slice: deepest(|p| p.ends_with(".slice")),
        container: container(&parts),
    }
}

fn container(parts: &[&str]) -> Option<Container> {
    const PREFIXES: [(&str, &str); 4] = [
        ("docker-", "docker"),
        ("cri-containerd-", "containerd"),
        ("crio-", "cri-o"),
        ("libpod-", "libpod"),
    ];
    let pod = parts.iter().find_map(|p| pod_uid(p));
    let in_kubepods = parts.iter().any(|p| p.starts_with("kubepods"));
    for (i, part) in parts.iter().enumerate().rev() {
        let stem = part.strip_suffix(".scope").unwrap_or(part);
        for (prefix, runtime) in PREFIXES {
            if let Some(id) = stem.strip_prefix(prefix).filter(|id| is_container_id(id)) {
                return Some(Container {
                    runtime,
                    id: id.to_string(),
                    pod,
                });
            }
        }
        // cgroup v1 names the container by a bare id under its runtime's directory.
        if is_container_id(stem) && i > 0 {
            let runtime = match parts[i - 1] {
                "docker" => "docker",
                _ if in_kubepods => "kubepods",
                _ if parts[..i].contains(&"containerd") => "containerd",
                _ => continue,
            };
            return Some(Container {
                runtime,
                id: stem.to_string(),
                pod,
            });
        }
    }
    None
}

/// `pod<uid>` (v1) or `kubepods-<qos>-pod<uid with underscores>.slice` (v2).
fn pod_uid(part: &str) -> Option<String> {
    let start = if part.starts_with("pod") {
        3
    } else {
        part.find("-pod")? + 4
    };
    let uid = part[start..]
        .strip_suffix(".slice")
        .unwrap_or(&part[start..]);
    (uid.len() == 36).then(|| uid.replace('_', "-"))
}

fn is_container_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn parse_status(content: &str) -> Status {
    let mut status = Status::default();
    for (key, value) in content.lines().filter_map(|l| l.split_once(':')) {
        let value = value.trim();
        match key {
            "Threads" => status.threads = value.parse().ok(),
            "CapEff" => status.cap_eff = u64::from_str_radix(value, 16).ok(),
            _ => {}
        }
    }
    status
}

/// Fields after the parenthesised command name, which may itself contain spaces
/// and parentheses.
pub fn parse_stat(content: &str) -> Option<Stat> {
    let after_comm = &content[content.rfind(')')? + 1..];
    // Index 0 here is field 3 (state) in proc(5).
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    Some(Stat {
        tty_nr: fields.get(4)?.parse::<i64>().ok()? as u32,
        nice: fields.get(16)?.parse().ok()?,
    })
}

pub fn parse_limit(content: &str, name: &str) -> Option<Limit> {
    let line = content.lines().find(|l| l.starts_with(name))?;
    let mut values = line[name.len()..].split_whitespace();
    Some(Limit {
        soft: values.next()?.to_string(),
        hard: values.next()?.to_string(),
    })
}

/// (Pss, Swap) in bytes.
pub fn parse_smaps_rollup(content: &str) -> (Option<u64>, Option<u64>) {
    let kb = |key: &str| {
        content.lines().find_map(|line| {
            let (k, v) = line.split_once(':')?;
            (k == key).then(|| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())?
        })
    };
    (kb("Pss").map(|v| v * 1024), kb("Swap").map(|v| v * 1024))
}

/// `None` for the "unset" value (u32::MAX), which processes outside a login session
/// inherit.
pub fn parse_loginuid(content: &str) -> Option<u32> {
    content.trim().parse().ok().filter(|&uid| uid != u32::MAX)
}

pub fn decode_capabilities(mask: u64) -> Capabilities {
    let known = (1u64 << CAPABILITIES.len()) - 1;
    if mask == 0 {
        Capabilities::None
    } else if mask & known == known {
        Capabilities::Full
    } else {
        Capabilities::Some(
            CAPABILITIES
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, name)| *name)
                .collect(),
        )
    }
}

/// Device number from `stat` to a terminal name, for the common majors.
pub fn tty_name(tty_nr: u32) -> Option<String> {
    if tty_nr == 0 {
        return None;
    }
    let major = (tty_nr >> 8) & 0xfff;
    let minor = (tty_nr & 0xff) | ((tty_nr >> 12) & 0xfff00);
    Some(match major {
        136..=143 => format!("pts/{}", minor + (major - 136) * 256),
        4 if minor < 64 => format!("tty{minor}"),
        4 => format!("ttyS{}", minor - 64),
        _ => format!("{major}:{minor}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCKER_ID: &str = "8d008d1c6d67a3f2b5e4c1d0f9e8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0";

    #[test]
    fn cgroup_v2_user_scope() {
        let cg = parse_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-ghostty-surface-transient-4873.scope\n");
        assert_eq!(
            cg.unit.as_deref(),
            Some("app-ghostty-surface-transient-4873.scope")
        );
        assert_eq!(cg.slice.as_deref(), Some("app.slice"));
        assert_eq!(cg.container, None);
    }

    #[test]
    fn cgroup_v2_system_service() {
        let cg = parse_cgroup("0::/system.slice/nginx.service\n");
        assert_eq!(cg.path, "/system.slice/nginx.service");
        assert_eq!(cg.unit.as_deref(), Some("nginx.service"));
        assert_eq!(cg.slice.as_deref(), Some("system.slice"));
    }

    #[test]
    fn cgroup_v2_docker_container() {
        let cg = parse_cgroup(&format!("0::/system.slice/docker-{DOCKER_ID}.scope\n"));
        let c = cg.container.expect("container");
        assert_eq!(
            (c.runtime, c.id.as_str(), c.pod),
            ("docker", DOCKER_ID, None)
        );
    }

    #[test]
    fn cgroup_v1_prefers_the_systemd_hierarchy() {
        let content = format!(
            "12:pids:/docker/{DOCKER_ID}\n4:memory:/docker/{DOCKER_ID}\n1:name=systemd:/docker/{DOCKER_ID}\n"
        );
        let cg = parse_cgroup(&content);
        assert_eq!(cg.path, format!("/docker/{DOCKER_ID}"));
        assert_eq!(cg.container.map(|c| c.runtime), Some("docker"));
    }

    #[test]
    fn cgroup_kubepods_containerd_and_podman() {
        let k8s = parse_cgroup(&format!(
            "0::/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod1a2b3c4d_5e6f_7a8b_9c0d_1e2f3a4b5c6d.slice/cri-containerd-{DOCKER_ID}.scope\n"
        ));
        let c = k8s.container.expect("container");
        assert_eq!(c.runtime, "containerd");
        assert_eq!(c.id, DOCKER_ID);
        assert_eq!(
            c.pod.as_deref(),
            Some("1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d")
        );

        let v1 = parse_cgroup(&format!(
            "1:name=systemd:/kubepods/besteffort/pod0f1e2d3c-4b5a-6978-8a9b-0c1d2e3f4a5b/{DOCKER_ID}\n"
        ));
        let c = v1.container.expect("container");
        assert_eq!(c.runtime, "kubepods");
        assert_eq!(
            c.pod.as_deref(),
            Some("0f1e2d3c-4b5a-6978-8a9b-0c1d2e3f4a5b")
        );

        let podman = parse_cgroup(&format!(
            "0::/machine.slice/libpod-{DOCKER_ID}.scope/container\n"
        ));
        assert_eq!(podman.container.map(|c| c.runtime), Some("libpod"));
    }

    #[test]
    fn status_threads_and_caps() {
        let status = "Name:\tnginx\nUid:\t0\t0\t0\t0\nThreads:\t12\nCapEff:\t0000000000000400\n";
        assert_eq!(
            parse_status(status),
            Status {
                threads: Some(12),
                cap_eff: Some(0x400)
            }
        );
    }

    #[test]
    fn stat_handles_spaces_and_parens_in_comm() {
        let stat = "1407028 (my (odd) cmd) S 1406995 1406995 1406995 34817 -1 4194304 136 0 0 0 0 0 0 0 20 -5 1 0 12650689";
        let s = parse_stat(stat).unwrap();
        assert_eq!((s.tty_nr, s.nice), (34817, -5));
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn limits_row() {
        let limits = "Limit                     Soft Limit           Hard Limit           Units     \nMax open files            1024                 524288               files     \nMax locked memory         unlimited            unlimited            bytes     \n";
        assert_eq!(
            parse_limit(limits, "Max open files"),
            Some(Limit {
                soft: "1024".into(),
                hard: "524288".into()
            })
        );
        assert_eq!(parse_limit(limits, "Max processes"), None);
    }

    #[test]
    fn smaps_rollup_in_bytes() {
        let smaps = "63150172d000-7ffeb2070000 ---p 00000000 00:00 0  [rollup]\nRss:                1832 kB\nPss:                 170 kB\nSwap:                  8 kB\nSwapPss:               8 kB\n";
        assert_eq!(
            parse_smaps_rollup(smaps),
            (Some(170 * 1024), Some(8 * 1024))
        );
    }

    #[test]
    fn loginuid_unset_is_none() {
        assert_eq!(parse_loginuid("1000"), Some(1000));
        assert_eq!(parse_loginuid("4294967295"), None);
    }

    #[test]
    fn capabilities_decode() {
        assert_eq!(decode_capabilities(0), Capabilities::None);
        assert_eq!(
            decode_capabilities(1 << 10 | 1 << 13),
            Capabilities::Some(vec!["net_bind_service", "net_raw"])
        );
        assert_eq!(decode_capabilities(0x1ff_ffff_ffff), Capabilities::Full);
        assert_eq!(decode_capabilities(u64::MAX), Capabilities::Full);
    }

    #[test]
    fn tty_names() {
        assert_eq!(tty_name(0), None);
        assert_eq!(tty_name(136 << 8 | 3), Some("pts/3".into()));
        assert_eq!(tty_name(4 << 8 | 1), Some("tty1".into()));
        assert_eq!(tty_name(4 << 8 | 65), Some("ttyS1".into()));
    }
}
