use std::cmp::Ordering;

use ratatui::widgets::TableState;

use crate::types::{ProcessInfo, ProcessSortBy};

const DASHBOARD_TOP: usize = 8;

#[derive(Default)]
pub struct ProcessView {
    pub sort_by: ProcessSortBy,
    pub sort_asc: bool,
    pub table_state: TableState,
    /// Indices into the collector rows: filtered, then sorted. Rebuilt on refresh or
    /// when the sort or filter changes, never per frame.
    pub order: Vec<usize>,
    /// Highest CPU users regardless of the table's sort and filter.
    pub top: Vec<usize>,
}

impl ProcessView {
    pub fn rebuild(&mut self, rows: &[ProcessInfo], filter: &str) {
        let needle = filter.to_lowercase();
        self.order.clear();
        self.order.extend(
            rows.iter()
                .enumerate()
                .filter(|(_, p)| matches(p, &needle))
                .map(|(i, _)| i),
        );
        let (sort_by, asc) = (self.sort_by.clone(), self.sort_asc);
        self.order.sort_unstable_by(|&a, &b| {
            let ord = compare(&rows[a], &rows[b], &sort_by);
            let ord = if asc { ord } else { ord.reverse() };
            ord.then(rows[a].pid().cmp(&rows[b].pid()))
        });

        self.top.clear();
        self.top.extend(0..rows.len());
        self.top.sort_unstable_by(|&a, &b| {
            rows[b]
                .cpu_usage
                .total_cmp(&rows[a].cpu_usage)
                .then(rows[a].pid().cmp(&rows[b].pid()))
        });
        self.top.truncate(DASHBOARD_TOP);
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

fn compare(a: &ProcessInfo, b: &ProcessInfo, sort_by: &ProcessSortBy) -> Ordering {
    match sort_by {
        ProcessSortBy::Pid => a.pid().cmp(&b.pid()),
        ProcessSortBy::Name => a.name_lower.cmp(&b.name_lower),
        ProcessSortBy::Cpu => a.cpu_usage.total_cmp(&b.cpu_usage),
        ProcessSortBy::Memory => a.memory.cmp(&b.memory),
    }
}

/// `needle` must already be lowercase.
fn matches(p: &ProcessInfo, needle: &str) -> bool {
    needle.is_empty()
        || p.name_lower.contains(needle)
        || p.command_lower.contains(needle)
        || pid_contains(p.pid(), needle)
}

fn pid_contains(pid: u32, needle: &str) -> bool {
    use std::io::Write;
    let mut buf = [0u8; 10];
    let mut cursor = &mut buf[..];
    let _ = write!(cursor, "{pid}");
    let len = 10 - cursor.len();
    std::str::from_utf8(&buf[..len]).is_ok_and(|s| s.contains(needle))
}
