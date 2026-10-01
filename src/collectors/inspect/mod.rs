#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

#[cfg(not(target_os = "linux"))]
use std::time::Duration;
use std::time::Instant;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, Uid, UpdateKind};

use super::ports::PortScanner;
use super::users::UserNames;
use crate::types::{PortInfo, ProcKey};

/// Why a field has no value. Kept apart so the inspector can say "denied" when
/// elevated rights would reveal it, rather than a bare dash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Denied,
    Unavailable,
}

pub type Field<T> = Result<T, Missing>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ProcessDetail {
    pub key: ProcKey,
    pub exe: Field<String>,
    pub argv: Field<Vec<String>>,
    pub cwd: Field<String>,
    pub root: Field<String>,
    pub environ: Field<Vec<String>>,
    pub user: Field<Account>,
    pub effective_user: Field<Account>,
    pub group: Field<Account>,
    pub session: Field<u32>,
    pub ports: Field<Vec<PortInfo>>,
    pub extras: Extras,
}

/// Platform-specific sections of the inspector.
#[derive(Debug, Clone)]
pub enum Extras {
    None,
    #[cfg(target_os = "linux")]
    Linux(Box<linux::Extras>),
    #[cfg(windows)]
    Windows(Box<windows::Extras>),
}

#[cfg(target_os = "linux")]
const EXTRAS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_os = "linux"))]
const PORT_SCAN_INTERVAL: Duration = Duration::from_secs(5);

/// Collects detail for the one process the inspector shows. It owns a separate
/// `System` because refreshing a single pid on the shared one recomputes every other
/// process's CPU% against the short interval since the last full refresh.
pub struct DetailCollector {
    sys: System,
    own_uid: Option<Uid>,
    target: Option<ProcKey>,
    #[cfg(not(target_os = "linux"))]
    ports_cache: Option<(Instant, Field<Vec<PortInfo>>)>,
    /// Extras read many small files, so they refresh less often than the tick.
    #[cfg(target_os = "linux")]
    extras_cache: Option<(ProcKey, linux::Extras)>,
    #[cfg(windows)]
    extras_cache: Option<(ProcKey, windows::Extras)>,
    #[cfg(windows)]
    services: windows::ServiceMap,
}

impl DetailCollector {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            own_uid: own_uid(),
            target: None,
            #[cfg(not(target_os = "linux"))]
            ports_cache: None,
            #[cfg(any(target_os = "linux", windows))]
            extras_cache: None,
            #[cfg(windows)]
            services: windows::ServiceMap::default(),
        }
    }

    pub fn set_target(&mut self, target: Option<ProcKey>) {
        #[cfg(not(target_os = "linux"))]
        if self.target != target {
            self.ports_cache = None;
        }
        self.target = target;
    }

    /// Returns `None` when the target is unset or no longer running.
    pub fn collect(
        &mut self,
        users: &mut UserNames,
        scanner: Option<&mut Box<dyn PortScanner>>,
    ) -> Option<ProcessDetail> {
        let key = self.target?;
        let pid = Pid::from_u32(key.pid);
        let kind = ProcessRefreshKind::nothing()
            .with_user(UpdateKind::Always)
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always)
            .with_cwd(UpdateKind::Always)
            .with_root(UpdateKind::Always)
            .with_environ(UpdateKind::Always)
            .without_tasks();
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
        let process = self.sys.process(pid)?;
        if process.start_time() != key.start_time {
            return None;
        }

        let now = Instant::now();
        let account = |users: &mut UserNames, uid: Option<&Uid>| {
            uid.map(|uid| Account {
                name: users.resolve(Some(uid), now).to_string(),
                id: uid.to_string(),
            })
            .ok_or(Missing::Unavailable)
        };
        // sysinfo reports unreadable fields as empty. Another account's process is the
        // usual reason, so call those denied rather than absent.
        let why = if process.user_id().is_some() && process.user_id() == self.own_uid.as_ref() {
            Missing::Unavailable
        } else {
            Missing::Denied
        };
        let mut detail = ProcessDetail {
            key,
            exe: present(process.exe().map(|p| p.display().to_string()), why),
            argv: nonempty(process.cmd().iter().map(lossy).collect(), why),
            cwd: present(process.cwd().map(|p| p.display().to_string()), why),
            root: present(process.root().map(|p| p.display().to_string()), why),
            environ: nonempty(process.environ().iter().map(lossy).collect(), why),
            user: account(users, process.user_id()),
            effective_user: account(users, process.effective_user_id()),
            group: process
                .group_id()
                .map(|gid| Account {
                    name: users.group(&gid).unwrap_or("-").to_string(),
                    id: gid.to_string(),
                })
                .ok_or(Missing::Unavailable),
            session: process
                .session_id()
                .map(|s| s.as_u32())
                .ok_or(Missing::Unavailable),
            ports: Err(Missing::Unavailable),
            extras: Extras::None,
        };

        #[cfg(target_os = "linux")]
        {
            let _ = scanner;
            linux::read_common(key.pid, &mut detail);
            let stale = match &self.extras_cache {
                Some((cached, extras)) => {
                    *cached != key || extras.collected.elapsed() >= EXTRAS_INTERVAL
                }
                None => true,
            };
            if stale {
                self.extras_cache = Some((key, linux::read_extras(key.pid, users)));
            }
            if let Some((_, extras)) = &self.extras_cache {
                detail.extras = Extras::Linux(Box::new(extras.clone()));
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            detail.ports = self.ports_by_scan(key.pid, scanner, now);
        }
        #[cfg(windows)]
        {
            let stale = match &self.extras_cache {
                Some((cached, extras)) => *cached != key || windows::is_stale(extras),
                None => true,
            };
            if stale {
                let extras = windows::read_extras(key.pid, &mut self.services);
                self.extras_cache = Some((key, extras));
            }
            if let Some((_, extras)) = &self.extras_cache {
                detail.extras = Extras::Windows(Box::new(extras.clone()));
            }
        }
        Some(detail)
    }

    /// Without per-process socket tables, owned ports come from a full scan filtered
    /// by pid. That scan can be slow (lsof on macOS), so its result is reused briefly.
    #[cfg(not(target_os = "linux"))]
    fn ports_by_scan(
        &mut self,
        pid: u32,
        scanner: Option<&mut Box<dyn PortScanner>>,
        now: Instant,
    ) -> Field<Vec<PortInfo>> {
        if let Some((at, ports)) = &self.ports_cache {
            if now.duration_since(*at) < PORT_SCAN_INTERVAL {
                return ports.clone();
            }
        }
        let ports = scanner
            .ok_or(Missing::Unavailable)?
            .scan()
            .map(|all| all.into_iter().filter(|p| p.pid == Some(pid)).collect())
            .map_err(|_| Missing::Unavailable);
        self.ports_cache = Some((now, ports.clone()));
        ports
    }
}

fn lossy(s: &std::ffi::OsString) -> String {
    s.to_string_lossy().into_owned()
}

fn present<T>(value: Option<T>, why: Missing) -> Field<T> {
    value.ok_or(why)
}

fn nonempty<T>(items: Vec<T>, why: Missing) -> Field<Vec<T>> {
    if items.is_empty() {
        Err(why)
    } else {
        Ok(items)
    }
}

fn own_uid() -> Option<Uid> {
    let pid = sysinfo::get_current_pid().ok()?;
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing()
        .with_user(UpdateKind::Always)
        .without_tasks();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, kind);
    sys.process(pid)?.user_id().cloned()
}
