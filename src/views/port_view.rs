use ratatui::widgets::TableState;

use crate::types::{PortInfo, PortSortBy};

pub struct PortView {
    pub sort_by: PortSortBy,
    pub sort_asc: bool,
    pub table_state: TableState,
}

impl Default for PortView {
    fn default() -> Self {
        Self {
            sort_by: PortSortBy::default(),
            sort_asc: true,
            table_state: TableState::default(),
        }
    }
}

impl PortView {
    pub fn sort(&self, ports: &mut [PortInfo]) {
        ports.sort_by(|a, b| {
            let ord = match self.sort_by {
                PortSortBy::Port => a.local_port.cmp(&b.local_port),
                PortSortBy::Protocol => a.protocol.cmp(&b.protocol),
                PortSortBy::Pid => a.pid.cmp(&b.pid),
                PortSortBy::State => a.state.cmp(&b.state),
            };
            if self.sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    pub fn filtered<'a>(ports: &'a [PortInfo], filter: &str) -> Vec<&'a PortInfo> {
        if filter.is_empty() {
            return ports.iter().collect();
        }
        let filter = filter.to_lowercase();
        ports
            .iter()
            .filter(|p| {
                p.local_port.to_string().contains(&filter)
                    || p.process_name.to_lowercase().contains(&filter)
                    || p.local_addr.contains(&filter)
                    || p.protocol.to_lowercase().contains(&filter)
            })
            .collect()
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
