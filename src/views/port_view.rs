use ratatui::widgets::TableState;

use crate::types::{PortInfo, PortSortBy};

pub struct PortView {
    pub sort_by: PortSortBy,
    pub sort_asc: bool,
    pub table_state: TableState,
    /// Indices into the scanned ports: filtered, then sorted.
    pub order: Vec<usize>,
}

impl Default for PortView {
    fn default() -> Self {
        Self {
            sort_by: PortSortBy::default(),
            sort_asc: true,
            table_state: TableState::default(),
            order: Vec::new(),
        }
    }
}

impl PortView {
    pub fn rebuild(&mut self, ports: &[PortInfo], filter: &str) {
        let needle = filter.to_lowercase();
        self.order.clear();
        self.order.extend(
            ports
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(p, &needle))
                .map(|(i, _)| i),
        );
        let (sort_by, asc) = (self.sort_by.clone(), self.sort_asc);
        self.order.sort_by(|&a, &b| {
            let (a, b) = (&ports[a], &ports[b]);
            let ord = match sort_by {
                PortSortBy::Port => a.local_port.cmp(&b.local_port),
                PortSortBy::Protocol => a.protocol.cmp(&b.protocol),
                PortSortBy::Pid => a.pid.cmp(&b.pid),
                PortSortBy::State => a.state.cmp(&b.state),
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    pub fn cycle_sort(&mut self) {
        self.sort_by = match self.sort_by {
            PortSortBy::Port => PortSortBy::Protocol,
            PortSortBy::Protocol => PortSortBy::Pid,
            PortSortBy::Pid => PortSortBy::State,
            PortSortBy::State => PortSortBy::Port,
        };
    }
}

fn matches(p: &PortInfo, needle: &str) -> bool {
    needle.is_empty()
        || p.local_port.to_string().contains(needle)
        || p.process_name.to_lowercase().contains(needle)
        || p.local_addr.contains(needle)
        || p.protocol.to_lowercase().contains(needle)
}
