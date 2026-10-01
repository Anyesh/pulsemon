use anyhow::{bail, Result};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, ThreadKind, UpdateKind};

use crate::types::ProcessInfo;

pub struct ProcessCollector {
    sys: System,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self { sys: System::new() }
    }

    pub fn refresh(&mut self) {
        // Tasks off: on Linux each thread would otherwise be listed as its own process,
        // and walking every /proc/<pid>/task is the largest per-tick cost.
        let kind = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_disk_usage()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .without_tasks();
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }

    pub fn processes(&self) -> Vec<ProcessInfo> {
        self.sys
            .processes()
            .values()
            .filter(|process| process.thread_kind() != Some(ThreadKind::Userland))
            .map(|process| ProcessInfo {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().to_string(),
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
                status: format!("{:?}", process.status()),
                command: process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect()
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
