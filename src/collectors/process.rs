use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use sysinfo::{
    Pid, Process, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, ThreadKind, Uid,
    UpdateKind, Users,
};

use crate::types::{ProcKey, ProcessInfo};

const USER_LIST_RETRY: Duration = Duration::from_secs(5);

pub struct ProcessCollector {
    sys: System,
    rows: Vec<ProcessInfo>,
    index: HashMap<ProcKey, usize>,
    seen: Vec<bool>,
    users: UserNames,
    last_refresh: Option<Instant>,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            rows: Vec::new(),
            index: HashMap::new(),
            seen: Vec::new(),
            users: UserNames::new(),
            last_refresh: None,
        }
    }

    pub fn refresh(&mut self) {
        let now = Instant::now();
        let elapsed = self.last_refresh.map(|t| now.duration_since(t));
        self.last_refresh = Some(now);

        // Tasks off: on Linux each thread would otherwise be listed as its own process,
        // and walking every /proc/<pid>/task is the largest per-tick cost.
        let kind = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_disk_usage()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet)
            .without_tasks();
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);

        self.seen.clear();
        self.seen.resize(self.rows.len(), false);

        for process in self.sys.processes().values() {
            if process.thread_kind() == Some(ThreadKind::Userland) {
                continue;
            }
            let key = ProcKey {
                pid: process.pid().as_u32(),
                start_time: process.start_time(),
            };
            match self.index.get(&key) {
                Some(&i) => {
                    let row = &mut self.rows[i];
                    // A process that called exec keeps its pid and start time but changes
                    // name and command line, so the cached strings must be rebuilt.
                    if process.name().as_encoded_bytes() != row.name.as_bytes() {
                        set_identity(row, process);
                    }
                    update_dynamic(row, process, elapsed);
                    self.seen[i] = true;
                }
                None => {
                    let mut row = ProcessInfo {
                        key,
                        parent: None,
                        name: Arc::from(""),
                        name_lower: Arc::from(""),
                        command: Arc::from(""),
                        command_lower: Arc::from(""),
                        user: self.users.resolve(process.user_id(), now),
                        cpu_usage: 0.0,
                        memory: 0,
                        disk_rate: 0,
                        status: "",
                    };
                    set_identity(&mut row, process);
                    update_dynamic(&mut row, process, elapsed);
                    self.rows.push(row);
                    self.seen.push(true);
                }
            }
        }

        let mut i = 0;
        self.rows.retain(|_| {
            let keep = self.seen[i];
            i += 1;
            keep
        });
        self.index.clear();
        self.index
            .extend(self.rows.iter().enumerate().map(|(i, row)| (row.key, i)));
    }

    pub fn rows(&self) -> &[ProcessInfo] {
        &self.rows
    }

    pub fn kill_process(&self, pid: u32) -> Result<()> {
        let sysinfo_pid = Pid::from_u32(pid);
        let process = self
            .sys
            .process(sysinfo_pid)
            .ok_or_else(|| anyhow::anyhow!("Process with PID {} not found", pid))?;

        if !process.kill() {
            bail!("Failed to kill process with PID {}", pid);
        }

        Ok(())
    }
}

fn set_identity(row: &mut ProcessInfo, process: &Process) {
    let name = process.name().to_string_lossy();
    row.name_lower = Arc::from(name.to_lowercase());
    row.name = Arc::from(name);
    let command = join_args(process.cmd());
    row.command_lower = Arc::from(command.to_lowercase());
    row.command = Arc::from(command);
}

fn update_dynamic(row: &mut ProcessInfo, process: &Process, elapsed: Option<Duration>) {
    row.parent = process.parent().map(|p| p.as_u32());
    row.cpu_usage = process.cpu_usage();
    row.memory = process.memory();
    row.status = status_str(process.status());
    let disk = process.disk_usage();
    row.disk_rate = match elapsed {
        Some(dt) if !dt.is_zero() => {
            ((disk.read_bytes + disk.written_bytes) as f64 / dt.as_secs_f64()) as u64
        }
        _ => 0,
    };
}

fn join_args(args: &[std::ffi::OsString]) -> String {
    let mut out = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&arg.to_string_lossy());
    }
    out
}

pub fn status_str(status: ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Idle => "Idle",
        ProcessStatus::Run => "Run",
        ProcessStatus::Sleep => "Sleep",
        ProcessStatus::Stop => "Stop",
        ProcessStatus::Zombie => "Zombie",
        ProcessStatus::Tracing => "Tracing",
        ProcessStatus::Dead => "Dead",
        ProcessStatus::Wakekill => "Wakekill",
        ProcessStatus::Waking => "Waking",
        ProcessStatus::Parked => "Parked",
        ProcessStatus::LockBlocked => "Locked",
        ProcessStatus::UninterruptibleDiskSleep => "DiskSleep",
        ProcessStatus::Suspended => "Suspended",
        ProcessStatus::Unknown(_) => "Unknown",
    }
}

/// Resolves user ids to names, caching hits. The `Users` list is reloaded after a
/// miss, but at most every few seconds, because a burst of new processes owned by
/// an unlisted account (LDAP users, Windows service SIDs) would otherwise reload it
/// once per process.
struct UserNames {
    users: Users,
    names: HashMap<Uid, Arc<str>>,
    last_list_refresh: Instant,
    unknown: Arc<str>,
}

impl UserNames {
    fn new() -> Self {
        Self {
            users: Users::new_with_refreshed_list(),
            names: HashMap::new(),
            last_list_refresh: Instant::now(),
            unknown: Arc::from("-"),
        }
    }

    fn resolve(&mut self, uid: Option<&Uid>, now: Instant) -> Arc<str> {
        let Some(uid) = uid else {
            return self.unknown.clone();
        };
        if let Some(name) = self.names.get(uid) {
            return name.clone();
        }
        if self.users.get_user_by_id(uid).is_none()
            && now.duration_since(self.last_list_refresh) >= USER_LIST_RETRY
        {
            self.users.refresh();
            self.last_list_refresh = now;
        }
        match self.users.get_user_by_id(uid) {
            Some(user) => {
                let name: Arc<str> = Arc::from(user.name());
                self.names.insert(uid.clone(), name.clone());
                name
            }
            None => Arc::from(uid.to_string()),
        }
    }
}
