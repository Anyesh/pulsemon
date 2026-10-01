use std::collections::HashSet;
use std::fs;
use std::io;

use super::{Field, Missing, ProcessDetail};
use crate::collectors::ports::linux::{parse_proc_net, pid_socket_inodes, TABLES};
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
