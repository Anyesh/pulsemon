use ratatui::widgets::TableState;

use crate::types::{ProcessInfo, ProcessSortBy};

#[derive(Default)]
pub struct ProcessView {
    pub sort_by: ProcessSortBy,
    pub sort_asc: bool,
    pub table_state: TableState,
}

impl ProcessView {
    pub fn sort(&self, procs: &mut [ProcessInfo]) {
        procs.sort_by(|a, b| {
            let ordering = match self.sort_by {
                ProcessSortBy::Pid => a.pid.cmp(&b.pid),
                ProcessSortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ProcessSortBy::Cpu => a
                    .cpu_usage
                    .partial_cmp(&b.cpu_usage)
                    .unwrap_or(std::cmp::Ordering::Equal),
                ProcessSortBy::Memory => a.memory.cmp(&b.memory),
            };
            if self.sort_asc {
                ordering
            } else {
                ordering.reverse()
            }
        });
    }

    pub fn filtered<'a>(procs: &'a [ProcessInfo], filter: &str) -> Vec<&'a ProcessInfo> {
        if filter.is_empty() {
            return procs.iter().collect();
        }
        let filter = filter.to_lowercase();
        procs
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&filter)
                    || p.command.to_lowercase().contains(&filter)
                    || p.pid.to_string().contains(&filter)
            })
            .collect()
    }

    pub fn cycle_sort(&mut self) {
        self.sort_by = match self.sort_by {
            ProcessSortBy::Cpu => ProcessSortBy::Memory,
            ProcessSortBy::Memory => ProcessSortBy::Pid,
            ProcessSortBy::Pid => ProcessSortBy::Name,
            ProcessSortBy::Name => ProcessSortBy::Cpu,
        };
    }

    pub fn sort_label(&self) -> String {
        format!(
            "Sort: {:?} ({})",
            self.sort_by,
            if self.sort_asc { "asc" } else { "desc" }
        )
    }
}
