use std::cmp::Ordering;
use std::net::IpAddr;

use super::{Sort, TableCursor};
use crate::types::PortInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortColumn {
    Protocol,
    Local,
    Port,
    Remote,
    State,
    Pid,
    Process,
}

impl PortColumn {
    pub const ALL: [Self; 7] = [
        Self::Protocol,
        Self::Local,
        Self::Port,
        Self::Remote,
        Self::State,
        Self::Pid,
        Self::Process,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "proto" | "protocol" => Self::Protocol,
            "local" | "addr" | "address" => Self::Local,
            "port" => Self::Port,
            "remote" => Self::Remote,
            "state" => Self::State,
            "pid" => Self::Pid,
            "process" | "proc" | "name" => Self::Process,
            _ => return None,
        })
    }
}

/// Identifies a socket across rescans: the 4-tuple plus protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortKey {
    protocol: String,
    local_addr: String,
    local_port: u16,
    remote_addr: String,
    remote_port: u16,
}

impl PortKey {
    fn of(p: &PortInfo) -> Self {
        Self {
            protocol: p.protocol.clone(),
            local_addr: p.local_addr.clone(),
            local_port: p.local_port,
            remote_addr: p.remote_addr.clone(),
            remote_port: p.remote_port,
        }
    }

    fn matches(&self, p: &PortInfo) -> bool {
        self.local_port == p.local_port
            && self.remote_port == p.remote_port
            && self.protocol == p.protocol
            && self.local_addr == p.local_addr
            && self.remote_addr == p.remote_addr
    }
}

pub struct PortView {
    pub sort: Sort<PortColumn>,
    pub filter: String,
    /// Indices into the scanned ports: filtered, then sorted.
    pub order: Vec<usize>,
    pub cursor: TableCursor,
    pub selected: Option<PortKey>,
}

impl Default for PortView {
    fn default() -> Self {
        Self {
            sort: Sort {
                column: PortColumn::Port,
                ascending: true,
            },
            filter: String::new(),
            order: Vec::new(),
            cursor: TableCursor::default(),
            selected: None,
        }
    }
}

impl PortView {
    pub fn rebuild(&mut self, ports: &[PortInfo]) {
        let needle = self.filter.to_lowercase();
        self.order.clear();
        self.order.extend(
            ports
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(p, &needle))
                .map(|(i, _)| i),
        );
        let sort = self.sort;
        self.order.sort_unstable_by(|&a, &b| {
            let (pa, pb) = (&ports[a], &ports[b]);
            let ord = compare(pa, pb, sort.column);
            let ord = if sort.ascending { ord } else { ord.reverse() };
            ord.then_with(|| tiebreak(pa, pb))
        });

        let found = self
            .selected
            .as_ref()
            .and_then(|key| self.order.iter().position(|&i| key.matches(&ports[i])));
        match found {
            Some(pos) => self.cursor.cursor = pos,
            None if self.selected.is_some() => {
                self.cursor.cursor = self.cursor.cursor.min(self.order.len().saturating_sub(1));
                self.sync_selected(ports);
            }
            None => {}
        }
        self.cursor.fit(self.order.len(), self.cursor.viewport);
    }

    pub fn move_by(&mut self, ports: &[PortInfo], delta: i64) {
        self.cursor.move_by(self.order.len(), delta);
        self.sync_selected(ports);
    }

    pub fn select_index(&mut self, ports: &[PortInfo], index: usize) {
        self.cursor.select(self.order.len(), index);
        self.sync_selected(ports);
    }

    fn sync_selected(&mut self, ports: &[PortInfo]) {
        self.selected = self
            .order
            .get(self.cursor.cursor)
            .map(|&i| PortKey::of(&ports[i]));
    }

    pub fn selected<'a>(&self, ports: &'a [PortInfo]) -> Option<&'a PortInfo> {
        self.selected.as_ref()?;
        ports.get(*self.order.get(self.cursor.cursor)?)
    }

    pub fn set_sort_column(&mut self, column: PortColumn) {
        self.sort = Sort {
            column,
            ascending: true,
        };
    }

    pub fn cycle_sort(&mut self) {
        let i = PortColumn::ALL
            .iter()
            .position(|&c| c == self.sort.column)
            .unwrap_or(0);
        self.set_sort_column(PortColumn::ALL[(i + 1) % PortColumn::ALL.len()]);
    }

    pub fn sort_label(&self) -> String {
        format!(
            "Sort: {:?} ({})",
            self.sort.column,
            if self.sort.ascending { "asc" } else { "desc" }
        )
    }
}

pub fn compare(a: &PortInfo, b: &PortInfo, column: PortColumn) -> Ordering {
    match column {
        PortColumn::Protocol => a.protocol.cmp(&b.protocol),
        PortColumn::Local => cmp_addr(&a.local_addr, &b.local_addr),
        PortColumn::Port => a.local_port.cmp(&b.local_port),
        PortColumn::Remote => {
            cmp_addr(&a.remote_addr, &b.remote_addr).then(a.remote_port.cmp(&b.remote_port))
        }
        PortColumn::State => a.state.cmp(&b.state),
        // Sockets without a known owner sort after every pid.
        PortColumn::Pid => match (a.pid, b.pid) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        PortColumn::Process => a
            .process_name
            .chars()
            .flat_map(char::to_lowercase)
            .cmp(b.process_name.chars().flat_map(char::to_lowercase)),
    }
}

fn tiebreak(a: &PortInfo, b: &PortInfo) -> Ordering {
    a.local_port
        .cmp(&b.local_port)
        .then_with(|| a.protocol.cmp(&b.protocol))
        .then_with(|| cmp_addr(&a.local_addr, &b.local_addr))
        .then_with(|| cmp_addr(&a.remote_addr, &b.remote_addr))
        .then(a.remote_port.cmp(&b.remote_port))
}

/// Numeric address order, so 10.0.0.2 sorts before 10.0.0.10; IPv4 before IPv6, and
/// anything unparsable (`*`) last.
fn cmp_addr(a: &str, b: &str) -> Ordering {
    match (a.parse::<IpAddr>(), b.parse::<IpAddr>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// `needle` must already be lowercase.
fn matches(p: &PortInfo, needle: &str) -> bool {
    let has = |field: &str| field.to_lowercase().contains(needle);
    needle.is_empty()
        || p.local_port.to_string().contains(needle)
        || p.pid.is_some_and(|pid| pid.to_string().contains(needle))
        || has(&p.process_name)
        || has(&p.local_addr)
        || has(&p.remote_addr)
        || has(&p.state)
        || has(&p.protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(proto: &str, local: &str, lport: u16, pid: Option<u32>, name: &str) -> PortInfo {
        PortInfo {
            protocol: proto.into(),
            local_addr: local.into(),
            local_port: lport,
            remote_addr: "0.0.0.0".into(),
            remote_port: 0,
            state: "LISTEN".into(),
            pid,
            process_name: name.into(),
        }
    }

    #[test]
    fn every_column_orders_rows() {
        let mut a = port("TCP", "10.0.0.2", 80, Some(5), "nginx");
        let mut b = port("UDP", "10.0.0.10", 53, None, "Avahi");
        a.remote_addr = "1.1.1.1".into();
        b.remote_addr = "8.8.8.8".into();
        a.state = "ESTABLISHED".into();
        let a_first = |col| compare(&a, &b, col) == Ordering::Less;
        assert!(a_first(PortColumn::Protocol));
        assert!(a_first(PortColumn::Local), "numeric, not lexical");
        assert!(!a_first(PortColumn::Port));
        assert!(a_first(PortColumn::Remote));
        assert!(a_first(PortColumn::State));
        assert!(a_first(PortColumn::Pid), "known pid before unknown");
        assert!(!a_first(PortColumn::Process), "case-insensitive");
    }

    #[test]
    fn selection_follows_socket_across_rescan() {
        let ports = vec![
            port("TCP", "0.0.0.0", 80, Some(1), "a"),
            port("TCP", "0.0.0.0", 22, Some(2), "b"),
        ];
        let mut view = PortView::default();
        view.rebuild(&ports);
        view.select_index(&ports, 1);
        assert_eq!(view.selected(&ports).map(|p| p.local_port), Some(80));

        let ports = vec![
            port("TCP", "0.0.0.0", 80, Some(1), "a"),
            port("TCP", "0.0.0.0", 443, Some(3), "c"),
            port("TCP", "0.0.0.0", 21, Some(4), "d"),
        ];
        view.rebuild(&ports);
        assert_eq!(view.cursor.cursor, 1);
        assert_eq!(view.selected(&ports).map(|p| p.local_port), Some(80));
    }
}
