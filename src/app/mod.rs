pub mod action;
mod command;
mod keymap;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::collectors::gpu::{self, GpuBackend};
use crate::collectors::ports::{self, PortScanner};
use crate::collectors::process::ProcessCollector;
use crate::collectors::system::SystemCollector;
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

    pub sys_collector: SystemCollector,
    pub proc_collector: ProcessCollector,
    pub port_scanner: Option<Box<dyn PortScanner>>,
    pub gpu_backends: Vec<Box<dyn GpuBackend>>,

    pub cpu_metrics: CpuMetrics,
    pub memory_metrics: MemoryMetrics,
    pub disk_metrics: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub ports: Vec<PortInfo>,
    pub gpu_metrics: Vec<GpuMetrics>,

    pub process_view: ProcessView,
    pub port_view: PortView,

    pub command_input: String,
    pub command_error: Option<String>,
    pub filter_input: String,
    pub confirm_kill: Option<(u32, String)>,
    pub show_help: bool,
    pub tick_rate: Duration,
    pub status_message: Option<(String, Instant)>,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let port_scanner = if config.no_ports {
            None
        } else {
            Some(ports::create_scanner())
        };

        let gpu_backends = if config.no_gpu {
            Vec::new()
        } else {
            gpu::detect_gpus()
        };

        let mut app = Self {
            running: true,
            view: View::Dashboard,
            input_mode: InputMode::Normal,
            sys_collector: SystemCollector::new(),
            proc_collector: ProcessCollector::new(),
            port_scanner,
            gpu_backends,
            cpu_metrics: CpuMetrics::default(),
            memory_metrics: MemoryMetrics::default(),
            disk_metrics: Vec::new(),
            processes: Vec::new(),
            ports: Vec::new(),
            gpu_metrics: Vec::new(),
            process_view: ProcessView::default(),
            port_view: PortView::default(),
            command_input: String::new(),
            command_error: None,
            filter_input: String::new(),
            confirm_kill: None,
            show_help: false,
            tick_rate: Duration::from_millis(config.rate),
            status_message: None,
        };

        app.refresh_all();
        app
    }

    pub fn refresh_all(&mut self) {
        self.sys_collector.refresh();
        self.cpu_metrics = self.sys_collector.cpu_metrics().clone();
        self.memory_metrics = self.sys_collector.memory_metrics().clone();
        self.disk_metrics = self.sys_collector.disk_metrics();

        self.proc_collector.refresh();
        self.processes = self.proc_collector.processes();
        self.process_view.sort(&mut self.processes);

        if let Some(scanner) = &mut self.port_scanner {
            if let Ok(ports) = scanner.scan() {
                self.ports = ports;
                self.port_view.sort(&mut self.ports);
            }
        }

        for backend in &mut self.gpu_backends {
            let _ = backend.refresh();
        }
        self.gpu_metrics = self.gpu_backends.iter().flat_map(|b| b.metrics()).collect();
    }

    pub fn filtered_processes(&self) -> Vec<&ProcessInfo> {
        ProcessView::filtered(&self.processes, &self.filter_input)
    }

    pub fn filtered_ports(&self) -> Vec<&PortInfo> {
        PortView::filtered(&self.ports, &self.filter_input)
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
            }
            Action::SwitchView(view) => self.view = view,
            Action::CycleView(step) => {
                let idx = (self.view.index() + VIEW_COUNT).wrapping_add_signed(step as isize);
                self.view = View::from_index(idx % VIEW_COUNT);
                self.filter_input.clear();
            }
            Action::MoveSelection(delta) => self.move_selection(delta),
            Action::SelectFirst => match self.view {
                View::ProcessTable => self.process_view.table_state.select(Some(0)),
                View::PortTable => self.port_view.table_state.select(Some(0)),
                _ => {}
            },
            Action::SelectLast => match self.view {
                View::ProcessTable => {
                    let len = self.filtered_processes().len();
                    select_last(&mut self.process_view.table_state, len);
                }
                View::PortTable => {
                    let len = self.filtered_ports().len();
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
                    self.port_view.sort(&mut self.ports);
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
                    self.port_view.sort(&mut self.ports);
                }
                _ => {}
            },
            Action::SortProcesses(col) => {
                self.process_view.sort_by = col;
                self.process_view.sort(&mut self.processes);
                self.set_status(format!("Sorting by: {:?}", self.process_view.sort_by));
            }
            Action::SortPorts(col) => {
                self.port_view.sort_by = col;
                self.port_view.sort(&mut self.ports);
                self.set_status(format!("Sorting by: {:?}", self.port_view.sort_by));
            }
            Action::RequestKill => self.initiate_kill(),
            Action::Kill(pid) => match self.proc_collector.kill_process(pid) {
                Ok(()) => self.set_status(format!("Killed PID {}", pid)),
                Err(e) => self.set_status(format!("Failed: {}", e)),
            },
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
                self.set_status(format!("Filter: {}", self.filter_input));
            }
        }
    }

    fn resort_processes(&mut self) {
        self.process_view.sort(&mut self.processes);
        self.set_status(self.process_view.sort_label());
    }

    fn move_selection(&mut self, delta: i32) {
        match self.view {
            View::ProcessTable => {
                let len = self.filtered_processes().len();
                move_selection(&mut self.process_view.table_state, len, delta);
            }
            View::PortTable => {
                let len = self.filtered_ports().len();
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
            _ => {}
        }
    }

    fn handle_confirm_kill(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            if let Some((pid, name)) = self.confirm_kill.take() {
                match self.proc_collector.kill_process(pid) {
                    Ok(()) => self.set_status(format!("Killed {} (PID {})", name, pid)),
                    Err(e) => self.set_status(format!("Failed to kill {}: {}", name, e)),
                }
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
            Some(pid) => match self.proc_collector.kill_process(pid) {
                Ok(()) => self.set_status(format!("Killed process on port {}", port)),
                Err(e) => self.set_status(format!("Failed: {}", e)),
            },
            None => self.set_status(format!("No process found on port {}", port)),
        }
    }

    fn initiate_kill(&mut self) {
        let target = match self.view {
            View::ProcessTable => self
                .process_view
                .table_state
                .selected()
                .and_then(|idx| self.filtered_processes().get(idx).copied())
                .map(|p| Ok((p.pid, p.name.clone()))),
            View::PortTable => self
                .port_view
                .table_state
                .selected()
                .and_then(|idx| self.filtered_ports().get(idx).copied())
                .map(|p| {
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
