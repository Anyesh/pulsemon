use anyhow::{bail, Result};
use sysinfo::{Pid, System};

use crate::types::{ProcessInfo, ProcessSortBy};

pub struct ProcessCollector {
    sys: System,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self { sys: System::new() }
    }

    pub fn refresh(&mut self) {
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    }

    pub fn processes(&self, sort_by: &ProcessSortBy, ascending: bool) -> Vec<ProcessInfo> {
        let mut procs: Vec<ProcessInfo> = self
            .sys
            .processes()
            .values()
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
            .collect();

        procs.sort_by(|a, b| {
            let ordering = match sort_by {
                ProcessSortBy::Pid => a.pid.cmp(&b.pid),
                ProcessSortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ProcessSortBy::Cpu => a
                    .cpu_usage
                    .partial_cmp(&b.cpu_usage)
                    .unwrap_or(std::cmp::Ordering::Equal),
                ProcessSortBy::Memory => a.memory.cmp(&b.memory),
            };
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });

        procs
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
