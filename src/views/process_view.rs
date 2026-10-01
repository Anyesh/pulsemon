use std::cmp::Ordering;

use super::{Sort, TableCursor};
use crate::types::{ProcKey, ProcessInfo};

const DASHBOARD_TOP: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessColumn {
    Pid,
    User,
    Name,
    Cpu,
    Memory,
    Disk,
    Status,
    Command,
}

impl ProcessColumn {
    pub const ALL: [Self; 8] = [
        Self::Pid,
        Self::User,
        Self::Name,
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::Status,
        Self::Command,
    ];

    /// Resource columns start with the biggest consumers on top.
    pub fn default_ascending(self) -> bool {
        !matches!(self, Self::Cpu | Self::Memory | Self::Disk)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "pid" => Self::Pid,
            "user" => Self::User,
            "name" => Self::Name,
            "cpu" => Self::Cpu,
            "mem" | "memory" => Self::Memory,
            "disk" | "io" => Self::Disk,
            "status" => Self::Status,
            "cmd" | "command" => Self::Command,
            _ => return None,
        })
    }
}

pub struct ProcessView {
    pub sort: Sort<ProcessColumn>,
    pub filter: String,
    /// Indices into the snapshot rows: filtered, then sorted. Rebuilt on refresh or
    /// when the sort or filter changes, never per frame.
    pub order: Vec<usize>,
    /// Highest CPU users regardless of the table's sort and filter.
    pub top: Vec<usize>,
    pub cursor: TableCursor,
    /// The selection is a process, not a row number, so it survives re-sorting.
    pub selected: Option<ProcKey>,
}

impl Default for ProcessView {
    fn default() -> Self {
        Self {
            sort: Sort {
                column: ProcessColumn::Cpu,
                ascending: false,
            },
            filter: String::new(),
            order: Vec::new(),
            top: Vec::new(),
            cursor: TableCursor::default(),
            selected: None,
        }
    }
}

impl ProcessView {
    pub fn rebuild(&mut self, rows: &[ProcessInfo]) {
        let needle = self.filter.to_lowercase();
        self.order.clear();
        self.order.extend(
            rows.iter()
                .enumerate()
                .filter(|(_, p)| matches(p, &needle))
                .map(|(i, _)| i),
        );
        let sort = self.sort;
        self.order.sort_unstable_by(|&a, &b| {
            let (pa, pb) = (&rows[a], &rows[b]);
            let ord = compare(pa, pb, sort.column);
            let ord = if sort.ascending { ord } else { ord.reverse() };
            // Many rows tie (hundreds of idle processes at 0.0%), and an unstable sort
            // would shuffle them every refresh without this.
            ord.then(pa.pid().cmp(&pb.pid()))
        });

        if let Some(key) = self.selected {
            match self.order.iter().position(|&i| rows[i].key == key) {
                Some(pos) => self.cursor.cursor = pos,
                None => {
                    // The process exited or was filtered out: stay at the same row
                    // position so the cursor lands on its neighbour.
                    self.cursor.cursor = self.cursor.cursor.min(self.order.len().saturating_sub(1));
                    self.sync_selected(rows);
                }
            }
        }
        self.cursor.fit(self.order.len(), self.cursor.viewport);

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

    pub fn move_by(&mut self, rows: &[ProcessInfo], delta: i64) {
        self.cursor.move_by(self.order.len(), delta);
        self.sync_selected(rows);
    }

    pub fn select_index(&mut self, rows: &[ProcessInfo], index: usize) {
        self.cursor.select(self.order.len(), index);
        self.sync_selected(rows);
    }

    pub fn select_key(&mut self, rows: &[ProcessInfo], key: ProcKey) {
        if let Some(pos) = self.order.iter().position(|&i| rows[i].key == key) {
            self.select_index(rows, pos);
        }
    }

    fn sync_selected(&mut self, rows: &[ProcessInfo]) {
        self.selected = self.order.get(self.cursor.cursor).map(|&i| rows[i].key);
    }

    pub fn set_sort_column(&mut self, column: ProcessColumn) {
        self.sort = Sort {
            column,
            ascending: column.default_ascending(),
        };
    }

    /// A second click on the active column flips the direction.
    pub fn click_column(&mut self, column: ProcessColumn) {
        if self.sort.column == column {
            self.sort.ascending = !self.sort.ascending;
        } else {
            self.set_sort_column(column);
        }
    }

    pub fn cycle_sort(&mut self) {
        let i = ProcessColumn::ALL
            .iter()
            .position(|&c| c == self.sort.column)
            .unwrap_or(0);
        self.set_sort_column(ProcessColumn::ALL[(i + 1) % ProcessColumn::ALL.len()]);
    }

    pub fn sort_label(&self) -> String {
        format!(
            "Sort: {:?} ({})",
            self.sort.column,
            if self.sort.ascending { "asc" } else { "desc" }
        )
    }
}

pub fn compare(a: &ProcessInfo, b: &ProcessInfo, column: ProcessColumn) -> Ordering {
    match column {
        ProcessColumn::Pid => a.pid().cmp(&b.pid()),
        ProcessColumn::User => a.user.cmp(&b.user),
        ProcessColumn::Name => a.name_lower.cmp(&b.name_lower),
        ProcessColumn::Cpu => a.cpu_usage.total_cmp(&b.cpu_usage),
        ProcessColumn::Memory => a.memory.cmp(&b.memory),
        ProcessColumn::Disk => a.disk_rate.cmp(&b.disk_rate),
        ProcessColumn::Status => a.status.cmp(b.status),
        ProcessColumn::Command => a.command_lower.cmp(&b.command_lower),
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

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use super::*;

    pub fn proc(pid: u32, name: &str) -> ProcessInfo {
        ProcessInfo {
            key: ProcKey {
                pid,
                start_time: 1000 + pid as u64,
            },
            parent: None,
            name: Arc::from(name),
            name_lower: Arc::from(name.to_lowercase()),
            command: Arc::from(format!("/bin/{name}")),
            command_lower: Arc::from(format!("/bin/{}", name.to_lowercase())),
            user: Arc::from("root"),
            cpu_usage: 0.0,
            memory: 0,
            disk_rate: 0,
            status: "Sleep",
        }
    }

    fn order_of(view: &ProcessView, rows: &[ProcessInfo]) -> Vec<u32> {
        view.order.iter().map(|&i| rows[i].pid()).collect()
    }

    #[test]
    fn every_column_orders_rows() {
        let mut a = proc(1, "alpha");
        let mut b = proc(2, "Beta");
        a.user = Arc::from("zed");
        b.user = Arc::from("amy");
        a.cpu_usage = 50.0;
        b.cpu_usage = 5.0;
        a.memory = 10;
        b.memory = 20;
        a.disk_rate = 300;
        b.disk_rate = 100;
        a.status = "Sleep";
        b.status = "Run";
        let expect_a_first = |col| compare(&a, &b, col) == Ordering::Less;
        assert!(expect_a_first(ProcessColumn::Pid));
        assert!(!expect_a_first(ProcessColumn::User));
        assert!(expect_a_first(ProcessColumn::Name), "case-insensitive");
        assert!(!expect_a_first(ProcessColumn::Cpu));
        assert!(expect_a_first(ProcessColumn::Memory));
        assert!(!expect_a_first(ProcessColumn::Disk));
        assert!(!expect_a_first(ProcessColumn::Status));
        assert!(expect_a_first(ProcessColumn::Command));
    }

    #[test]
    fn nan_cpu_sorts_without_panicking() {
        let mut a = proc(1, "a");
        a.cpu_usage = f32::NAN;
        let b = proc(2, "b");
        assert_ne!(compare(&a, &b, ProcessColumn::Cpu), Ordering::Equal);
    }

    #[test]
    fn ties_break_by_pid_in_both_directions() {
        let rows = vec![proc(30, "x"), proc(10, "x"), proc(20, "x")];
        let mut view = ProcessView::default();
        view.set_sort_column(ProcessColumn::Name);
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![10, 20, 30]);
        view.sort.ascending = false;
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![10, 20, 30]);
    }

    #[test]
    fn selection_follows_process_across_resort() {
        let mut rows = vec![proc(1, "a"), proc(2, "b"), proc(3, "c")];
        rows[0].cpu_usage = 30.0;
        rows[1].cpu_usage = 20.0;
        rows[2].cpu_usage = 10.0;
        let mut view = ProcessView::default();
        view.rebuild(&rows);
        view.select_index(&rows, 0);
        assert_eq!(view.selected.map(|k| k.pid), Some(1));

        rows[0].cpu_usage = 1.0;
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![2, 3, 1]);
        assert_eq!(view.cursor.cursor, 2);
        assert_eq!(view.selected.map(|k| k.pid), Some(1));
    }

    #[test]
    fn exited_process_leaves_cursor_on_neighbour() {
        let rows = vec![proc(1, "a"), proc(2, "b"), proc(3, "c")];
        let mut view = ProcessView::default();
        view.set_sort_column(ProcessColumn::Pid);
        view.rebuild(&rows);
        view.select_index(&rows, 2);

        let rows = vec![proc(1, "a"), proc(2, "b")];
        view.rebuild(&rows);
        assert_eq!(view.cursor.cursor, 1);
        assert_eq!(view.selected.map(|k| k.pid), Some(2));
    }

    #[test]
    fn reused_pid_is_not_the_same_selection() {
        let rows = vec![proc(1, "a"), proc(2, "b")];
        let mut view = ProcessView::default();
        view.set_sort_column(ProcessColumn::Name);
        view.rebuild(&rows);
        view.select_index(&rows, 1);

        let mut reborn = proc(2, "b");
        reborn.key.start_time += 60;
        let rows = vec![reborn, proc(0, "0")];
        view.rebuild(&rows);
        assert_eq!(view.selected, Some(rows[0].key));
    }

    #[test]
    fn filter_matches_name_command_and_pid() {
        let rows = vec![proc(1234, "Firefox"), proc(99, "bash")];
        let mut view = ProcessView {
            filter: "FIRE".into(),
            ..Default::default()
        };
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![1234]);
        view.filter = "/bin/ba".into();
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![99]);
        view.filter = "23".into();
        view.rebuild(&rows);
        assert_eq!(order_of(&view, &rows), vec![1234]);
    }

    #[test]
    fn header_click_flips_then_switches() {
        let mut view = ProcessView::default();
        view.click_column(ProcessColumn::Cpu);
        assert!(view.sort.ascending);
        view.click_column(ProcessColumn::Name);
        assert_eq!(view.sort.column, ProcessColumn::Name);
        assert!(view.sort.ascending);
        view.click_column(ProcessColumn::Memory);
        assert!(!view.sort.ascending);
    }
}
