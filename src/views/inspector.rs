use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::collectors::inspect::ProcessDetail;
use crate::types::{ProcKey, ProcessInfo};

const HISTORY: usize = 60;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub key: ProcKey,
    pub name: Arc<str>,
}

/// Why the ancestor chain stops before reaching the root of the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainEnd {
    /// Reached a process with no parent.
    Root,
    /// The parent pid is no longer running.
    ParentExited(u32),
    /// The parent pid now belongs to a process started after the child, so the
    /// real parent exited and its pid was reused.
    PidReused(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lineage {
    /// Nearest first.
    pub ancestors: Vec<Link>,
    pub end: ChainEnd,
    /// The direct parent is init or a subreaper. Either a service manager started the
    /// process, or whatever launched it exited and the process was re-parented.
    pub adopted: bool,
}

pub fn lineage(rows: &[ProcessInfo], key: ProcKey) -> Lineage {
    let by_pid: HashMap<u32, &ProcessInfo> = rows.iter().map(|p| (p.pid(), p)).collect();
    let mut ancestors = Vec::new();
    let mut end = ChainEnd::Root;
    let mut adopted = false;

    let mut current = by_pid.get(&key.pid).filter(|p| p.key == key).copied();
    while let Some(child) = current {
        let Some(ppid) = child.parent else {
            break;
        };
        let Some(&parent) = by_pid.get(&ppid) else {
            end = ChainEnd::ParentExited(ppid);
            break;
        };
        if parent.key.start_time > child.key.start_time {
            end = ChainEnd::PidReused(ppid);
            break;
        }
        if ancestors.is_empty() {
            adopted = is_reaper(parent);
        }
        if ancestors.len() >= MAX_DEPTH || parent.key == key {
            break;
        }
        ancestors.push(Link {
            key: parent.key,
            name: parent.name.clone(),
        });
        current = Some(parent);
    }
    Lineage {
        ancestors,
        end,
        adopted,
    }
}

/// Direct children, oldest first. A row whose parent pid matches but which started
/// before this process belongs to an earlier holder of the same pid.
pub fn children(rows: &[ProcessInfo], key: ProcKey) -> Vec<Link> {
    let mut kids: Vec<&ProcessInfo> = rows
        .iter()
        .filter(|p| p.parent == Some(key.pid) && p.key.start_time >= key.start_time)
        .filter(|p| p.key != key)
        .collect();
    kids.sort_by_key(|p| (p.key.start_time, p.pid()));
    kids.into_iter()
        .map(|p| Link {
            key: p.key,
            name: p.name.clone(),
        })
        .collect()
}

fn is_reaper(p: &ProcessInfo) -> bool {
    p.pid() == 1 || matches!(&*p.name, "systemd" | "init" | "launchd")
}

pub struct InspectorView {
    pub key: ProcKey,
    back: Vec<ProcKey>,
    pub detail: Option<ProcessDetail>,
    pub row: Option<ProcessInfo>,
    pub cpu: VecDeque<u64>,
    pub memory: VecDeque<u64>,
    pub lineage: Lineage,
    pub children: Vec<Link>,
    pub show_env: bool,
    /// Index into `links()`: ancestors, then children.
    pub link_cursor: usize,
    /// Set when the link cursor moves, so the next render scrolls it into view.
    pub reveal_link: bool,
    pub scroll: u16,
}

impl InspectorView {
    pub fn new(key: ProcKey, rows: &[ProcessInfo]) -> Self {
        let mut view = Self {
            key,
            back: Vec::new(),
            detail: None,
            row: None,
            cpu: VecDeque::with_capacity(HISTORY),
            memory: VecDeque::with_capacity(HISTORY),
            lineage: Lineage {
                ancestors: Vec::new(),
                end: ChainEnd::Root,
                adopted: false,
            },
            children: Vec::new(),
            show_env: false,
            link_cursor: 0,
            reveal_link: false,
            scroll: 0,
        };
        view.update(rows);
        view
    }

    /// Re-targets the inspector, remembering the current process for `back`.
    pub fn follow(&mut self, key: ProcKey, rows: &[ProcessInfo]) {
        let back = std::mem::take(&mut self.back);
        let previous = self.key;
        *self = Self::new(key, rows);
        self.back = back;
        self.back.push(previous);
    }

    /// Returns false when there is nothing to go back to.
    pub fn back(&mut self, rows: &[ProcessInfo]) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        let back = std::mem::take(&mut self.back);
        *self = Self::new(previous, rows);
        self.back = back;
        true
    }

    pub fn update(&mut self, rows: &[ProcessInfo]) {
        self.row = rows.iter().find(|p| p.key == self.key).cloned();
        if let Some(row) = &self.row {
            push(&mut self.cpu, row.cpu_usage.round() as u64);
            push(&mut self.memory, row.memory);
        }
        self.lineage = lineage(rows, self.key);
        self.children = children(rows, self.key);
        self.link_cursor = self.link_cursor.min(self.link_count().saturating_sub(1));
    }

    pub fn link_count(&self) -> usize {
        self.lineage.ancestors.len() + self.children.len()
    }

    pub fn link(&self, index: usize) -> Option<&Link> {
        let ancestors = self.lineage.ancestors.len();
        if index < ancestors {
            self.lineage.ancestors.get(index)
        } else {
            self.children.get(index - ancestors)
        }
    }

    pub fn move_link(&mut self, delta: i64) {
        let count = self.link_count() as i64;
        if count > 0 {
            self.link_cursor = (self.link_cursor as i64 + delta).clamp(0, count - 1) as usize;
            self.reveal_link = true;
        }
    }

    pub fn set_detail(&mut self, detail: ProcessDetail) {
        if detail.key == self.key {
            self.detail = Some(detail);
        }
    }
}

fn push(history: &mut VecDeque<u64>, value: u64) {
    if history.len() >= HISTORY {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::process_view::tests::proc;

    fn row(pid: u32, parent: Option<u32>, start: u64, name: &str) -> ProcessInfo {
        let mut p = proc(pid, name);
        p.parent = parent;
        p.key.start_time = start;
        p
    }

    fn pids(links: &[Link]) -> Vec<u32> {
        links.iter().map(|l| l.key.pid).collect()
    }

    #[test]
    fn walks_ancestors_to_the_root() {
        let rows = vec![
            row(1, None, 10, "systemd"),
            row(900, Some(1), 20, "sshd"),
            row(1200, Some(900), 30, "bash"),
            row(1300, Some(1200), 40, "vim"),
        ];
        let l = lineage(&rows, rows[3].key);
        assert_eq!(pids(&l.ancestors), vec![1200, 900, 1]);
        assert_eq!(l.end, ChainEnd::Root);
        assert!(!l.adopted);
    }

    #[test]
    fn parent_started_after_child_is_pid_reuse() {
        let rows = vec![
            row(1, None, 10, "systemd"),
            row(500, Some(1), 90, "unrelated"),
            row(700, Some(500), 50, "orphan"),
        ];
        let l = lineage(&rows, rows[2].key);
        assert!(l.ancestors.is_empty());
        assert_eq!(l.end, ChainEnd::PidReused(500));
    }

    #[test]
    fn missing_parent_is_reported_as_exited() {
        let rows = vec![row(700, Some(650), 50, "worker")];
        let l = lineage(&rows, rows[0].key);
        assert_eq!(l.end, ChainEnd::ParentExited(650));
    }

    #[test]
    fn child_of_init_or_subreaper_is_marked_adopted() {
        let rows = vec![
            row(1, None, 10, "systemd"),
            row(2000, Some(1), 20, "daemon"),
            row(3000, Some(1), 5, "user-systemd"),
            row(3100, Some(3000), 30, "systemd"),
            row(3200, Some(3100), 40, "app"),
        ];
        assert!(lineage(&rows, rows[1].key).adopted);
        let mut user_manager = rows.clone();
        user_manager[3].key.start_time = 25;
        assert!(lineage(&user_manager, user_manager[4].key).adopted);
    }

    #[test]
    fn children_skip_rows_from_an_earlier_pid_holder() {
        let rows = vec![
            row(100, Some(1), 50, "parent"),
            row(150, Some(100), 80, "late"),
            row(120, Some(100), 60, "early"),
            row(110, Some(100), 40, "stale"),
            row(130, Some(999), 70, "other"),
        ];
        assert_eq!(pids(&children(&rows, rows[0].key)), vec![120, 150]);
    }

    #[test]
    fn cycles_terminate() {
        let rows = vec![row(5, Some(6), 10, "a"), row(6, Some(5), 10, "b")];
        let l = lineage(&rows, rows[0].key);
        assert!(l.ancestors.len() <= MAX_DEPTH);
    }

    #[test]
    fn follow_and_back_keep_a_history() {
        let rows = vec![row(1, None, 10, "init"), row(2, Some(1), 20, "child")];
        let mut view = InspectorView::new(rows[1].key, &rows);
        view.follow(rows[0].key, &rows);
        assert_eq!(view.key.pid, 1);
        assert!(view.back(&rows));
        assert_eq!(view.key.pid, 2);
        assert!(!view.back(&rows));
    }
}
