pub mod action;
mod command;
mod keymap;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::collectors::worker::{CollectorHandle, Request, Snapshot};
use crate::config::Config;
use crate::types::*;
use crate::views::port_view::PortView;
use crate::views::process_view::ProcessView;
use crate::views::{move_selection, select_last};
use action::Action;

const STATUS_TTL: Duration = Duration::from_secs(5);
const VIEW_COUNT: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    CpuDetail,
    MemoryDetail,
    DiskDetail,
    GpuDetail,
    ProcessTable,
    PortTable,
}

impl View {
    pub fn index(&self) -> usize {
        match self {
            View::Dashboard => 0,
            View::CpuDetail => 1,
            View::MemoryDetail => 2,
            View::DiskDetail => 3,
            View::GpuDetail => 4,
            View::ProcessTable => 5,
            View::PortTable => 6,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => View::CpuDetail,
            2 => View::MemoryDetail,
            3 => View::DiskDetail,
            4 => View::GpuDetail,
            5 => View::ProcessTable,
            6 => View::PortTable,
            _ => View::Dashboard,
        }
    }

    pub fn titles() -> &'static [&'static str] {
        &[
            "Dashboard",
            "CPU",
            "Memory",
            "Disk",
            "GPU",
            "Processes",
            "Ports",
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    CommandPalette,
    Filter,
    ConfirmKill,
}

pub struct App {
    pub running: bool,
    pub view: View,
    pub input_mode: InputMode,

    pub collector: CollectorHandle,
    refresh_in_flight: bool,
    pub data: Snapshot,
    /// Kept apart from `data` because most snapshots skip the port scan.
    pub ports: Vec<PortInfo>,

    pub process_view: ProcessView,
    pub port_view: PortView,

    pub command_input: String,
    pub command_error: Option<String>,
    pub filter_input: String,
    pub confirm_kill: Option<(u32, String)>,
    pub show_help: bool,
    pub tick_rate: Duration,
    pub status_message: Option<(String, Instant)>,
    /// Collector-thread time of the last refresh, kept only when `--debug-timing` is set.
    pub tick_cost: Option<Duration>,
    debug_timing: bool,
}

impl App {
    pub fn new(config: &Config, collector: CollectorHandle) -> Self {
        let mut app = Self {
            running: true,
            view: View::Dashboard,
            input_mode: InputMode::Normal,
            collector,
            refresh_in_flight: false,
            data: Snapshot::default(),
            ports: Vec::new(),
            process_view: ProcessView::default(),
            port_view: PortView::default(),
            command_input: String::new(),
            command_error: None,
            filter_input: String::new(),
            confirm_kill: None,
            show_help: false,
            tick_rate: Duration::from_millis(config.rate),
            status_message: None,
            tick_cost: None,
            debug_timing: config.debug_timing,
        };

        app.request_refresh();
        app
    }

    /// Asks the collector for fresh data unless a refresh is still running, so a
    /// refresh slower than the tick rate cannot queue up behind itself.
    pub fn request_refresh(&mut self) {
        if self.refresh_in_flight {
            return;
        }
        self.refresh_in_flight = true;
        // The full socket scan is only worth its cost while someone is looking at it.
        let ports = self.view == View::PortTable;
        self.collector.send(Request::Refresh { ports });
    }

    pub fn apply_snapshot(&mut self, mut snapshot: Snapshot) {
        self.refresh_in_flight = false;
        if let Some(ports) = snapshot.ports.take() {
            self.ports = ports;
        }
        if self.debug_timing {
            self.tick_cost = Some(snapshot.cost);
        }
        self.data = snapshot;
        self.rebuild_views();
    }

    fn rebuild_views(&mut self) {
        self.process_view
            .rebuild(&self.data.processes, &self.filter_input);
        self.port_view.rebuild(&self.ports, &self.filter_input);
    }

    pub fn selected_process(&self) -> Option<&ProcessInfo> {
        let idx = self.process_view.table_state.selected()?;
        let row = *self.process_view.order.get(idx)?;
        self.data.processes.get(row)
    }

    pub fn selected_port(&self) -> Option<&PortInfo> {
        let idx = self.port_view.table_state.selected()?;
        self.ports.get(*self.port_view.order.get(idx)?)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match self.input_mode {
            InputMode::ConfirmKill => self.handle_confirm_kill(key),
            InputMode::CommandPalette => self.handle_command_input(key),
            InputMode::Filter => self.handle_filter_input(key),
            InputMode::Normal if self.show_help => {
                if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc) {
                    self.show_help = false;
                }
            }
            InputMode::Normal => {
                if let Some(action) = keymap::normal(key) {
                    self.apply(action);
                }
            }
        }
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.running = false,
            Action::Back => {
                if self.view != View::Dashboard {
                    self.view = View::Dashboard;
                    self.filter_input.clear();
                    self.rebuild_views();
                } else {
                    self.running = false;
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::OpenPalette => {
                self.input_mode = InputMode::CommandPalette;
                self.command_input.clear();
                self.command_error = None;
            }
            Action::OpenFilter => {
                self.input_mode = InputMode::Filter;
                self.filter_input.clear();
                self.rebuild_views();
            }
            Action::SwitchView(view) => self.switch_view(view),
            Action::CycleView(step) => {
                let idx = (self.view.index() + VIEW_COUNT).wrapping_add_signed(step as isize);
                self.switch_view(View::from_index(idx % VIEW_COUNT));
                self.filter_input.clear();
                self.rebuild_views();
            }
            Action::MoveSelection(delta) => self.move_selection(delta),
            Action::SelectFirst => match self.view {
                View::ProcessTable => self.process_view.table_state.select(Some(0)),
                View::PortTable => self.port_view.table_state.select(Some(0)),
                _ => {}
            },
            Action::SelectLast => match self.view {
                View::ProcessTable => {
                    let len = self.process_view.order.len();
                    select_last(&mut self.process_view.table_state, len);
                }
                View::PortTable => {
                    let len = self.port_view.order.len();
                    select_last(&mut self.port_view.table_state, len);
                }
                _ => {}
            },
            Action::CycleSort => match self.view {
                View::ProcessTable => {
                    self.process_view.cycle_sort();
                    self.resort_processes();
                }
                View::PortTable => {
                    self.port_view.cycle_sort();
                    self.rebuild_views();
                }
                _ => {}
            },
            Action::ToggleSortDir => match self.view {
                View::ProcessTable => {
                    self.process_view.sort_asc = !self.process_view.sort_asc;
                    self.resort_processes();
                }
                View::PortTable => {
                    self.port_view.sort_asc = !self.port_view.sort_asc;
                    self.rebuild_views();
                }
                _ => {}
            },
            Action::SortProcesses(col) => {
                self.process_view.sort_by = col;
                self.rebuild_views();
                self.set_status(format!("Sorting by: {:?}", self.process_view.sort_by));
            }
            Action::SortPorts(col) => {
                self.port_view.sort_by = col;
                self.rebuild_views();
                self.set_status(format!("Sorting by: {:?}", self.port_view.sort_by));
            }
            Action::RequestKill => self.initiate_kill(),
            Action::Kill(pid) => self.collector.send(Request::Kill {
                pid,
                label: "process".into(),
            }),
            Action::KillPort(port) => self.kill_by_port(port),
            Action::AdjustRate(delta_ms) => {
                let ms = self.tick_rate.as_millis() as i64 + delta_ms;
                if (250..=10_000).contains(&ms) {
                    self.tick_rate = Duration::from_millis(ms as u64);
                    self.set_status(format!("Refresh rate: {}ms", ms));
                }
            }
            Action::SetRate(ms) => {
                self.tick_rate = Duration::from_millis(ms);
                self.set_status(format!("Refresh rate: {}ms", ms));
            }
            Action::SetFilter(filter) => {
                self.filter_input = filter;
                self.rebuild_views();
                self.set_status(format!("Filter: {}", self.filter_input));
            }
        }
    }

    fn switch_view(&mut self, view: View) {
        let entering_ports = view == View::PortTable && self.view != View::PortTable;
        self.view = view;
        if entering_ports {
            // Ports are not scanned while hidden, so fetch them now instead of
            // showing a stale list until the next tick.
            self.collector.send(Request::Refresh { ports: true });
        }
    }

    fn resort_processes(&mut self) {
        self.rebuild_views();
        self.set_status(self.process_view.sort_label());
    }

    fn move_selection(&mut self, delta: i32) {
        match self.view {
            View::ProcessTable => {
                let len = self.process_view.order.len();
                move_selection(&mut self.process_view.table_state, len, delta);
            }
            View::PortTable => {
                let len = self.port_view.order.len();
                move_selection(&mut self.port_view.table_state, len, delta);
            }
            _ => {}
        }
    }

    fn handle_command_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.command_input.clear();
                self.command_error = None;
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                let line = std::mem::take(&mut self.command_input);
                match command::parse(&line) {
                    Ok(Some(action)) => self.apply(action),
                    Ok(None) => {}
                    Err(msg) => self.set_status(msg),
                }
            }
            KeyCode::Backspace => {
                self.command_input.pop();
                self.command_error = None;
            }
            KeyCode::Char(c) => {
                self.command_input.push(c);
                self.command_error = None;
            }
            _ => {}
        }
    }

    fn handle_filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.filter_input.clear();
            }
            KeyCode::Enter => self.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.filter_input.pop();
            }
            KeyCode::Char(c) => self.filter_input.push(c),
            _ => return,
        }
        self.rebuild_views();
    }

    fn handle_confirm_kill(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            if let Some((pid, label)) = self.confirm_kill.take() {
                self.collector.send(Request::Kill { pid, label });
            }
        }
        self.confirm_kill = None;
        self.input_mode = InputMode::Normal;
    }

    fn kill_by_port(&mut self, port: u16) {
        let pid = self
            .ports
            .iter()
            .find(|p| p.local_port == port)
            .and_then(|p| p.pid);
        match pid {
            Some(pid) => self.collector.send(Request::Kill {
                pid,
                label: format!("process on port {port}"),
            }),
            None => self.set_status(format!("No process found on port {}", port)),
        }
    }

    fn initiate_kill(&mut self) {
        let target = match self.view {
            View::ProcessTable => self
                .selected_process()
                .map(|p| Ok((p.pid(), p.name.to_string()))),
            View::PortTable => self.selected_port().map(|p| {
                p.pid
                    .map(|pid| (pid, format!("port:{}", p.local_port)))
                    .ok_or(())
            }),
            _ => None,
        };
        match target {
            Some(Ok(target)) => {
                self.confirm_kill = Some(target);
                self.input_mode = InputMode::ConfirmKill;
            }
            Some(Err(())) => self.set_status("No PID associated with this port".to_string()),
            None => {}
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now() + STATUS_TTL));
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status_message.as_ref().map(|(msg, _)| msg.as_str())
    }

    pub fn status_deadline(&self) -> Option<Instant> {
        self.status_message.as_ref().map(|(_, until)| *until)
    }

    /// Returns true when a status message was cleared, so the caller knows to redraw.
    pub fn expire_status(&mut self, now: Instant) -> bool {
        match self.status_deadline() {
            Some(until) if now >= until => {
                self.status_message = None;
                true
            }
            _ => false,
        }
    }
}
