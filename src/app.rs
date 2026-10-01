use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::widgets::TableState;

use crate::collectors::gpu::{self, GpuBackend};
use crate::collectors::ports::{self, PortScanner};
use crate::collectors::process::ProcessCollector;
use crate::collectors::system::SystemCollector;
use crate::config::Config;
use crate::types::*;

const STATUS_TTL: Duration = Duration::from_secs(5);

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
            0 => View::Dashboard,
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

    // Collectors
    pub sys_collector: SystemCollector,
    pub proc_collector: ProcessCollector,
    pub port_scanner: Option<Box<dyn PortScanner>>,
    pub gpu_backends: Vec<Box<dyn GpuBackend>>,

    // Metrics
    pub cpu_metrics: CpuMetrics,
    pub memory_metrics: MemoryMetrics,
    pub disk_metrics: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub ports: Vec<PortInfo>,
    pub gpu_metrics: Vec<GpuMetrics>,

    // Table states
    pub process_table_state: TableState,
    pub port_table_state: TableState,

    // Sorting
    pub process_sort_by: ProcessSortBy,
    pub process_sort_asc: bool,
    pub port_sort_by: PortSortBy,
    pub port_sort_asc: bool,

    // Command palette
    pub command_input: String,
    pub command_error: Option<String>,

    // Filter
    pub filter_input: String,

    // Kill confirmation
    pub confirm_kill: Option<(u32, String)>,

    // Help
    pub show_help: bool,

    // Config
    pub tick_rate: Duration,

    // Status message
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
            process_table_state: TableState::default(),
            port_table_state: TableState::default(),
            process_sort_by: ProcessSortBy::default(),
            process_sort_asc: false,
            port_sort_by: PortSortBy::default(),
            port_sort_asc: true,
            command_input: String::new(),
            command_error: None,
            filter_input: String::new(),
            confirm_kill: None,
            show_help: false,
            tick_rate: Duration::from_millis(config.rate),
            status_message: None,
        };

        // Initial data collection
        app.refresh_all();
        app
    }

    pub fn refresh_all(&mut self) {
        self.sys_collector.refresh();
        self.cpu_metrics = self.sys_collector.cpu_metrics().clone();
        self.memory_metrics = self.sys_collector.memory_metrics().clone();
        self.disk_metrics = self.sys_collector.disk_metrics();

        self.proc_collector.refresh();
        self.processes = self
            .proc_collector
            .processes(&self.process_sort_by, self.process_sort_asc);

        if let Some(scanner) = &mut self.port_scanner {
            if let Ok(ports) = scanner.scan() {
                self.ports = ports;
                self.sort_ports();
            }
        }

        for backend in &mut self.gpu_backends {
            let _ = backend.refresh();
        }
        self.gpu_metrics = self.gpu_backends.iter().flat_map(|b| b.metrics()).collect();
    }

    fn sort_ports(&mut self) {
        let asc = self.port_sort_asc;
        self.ports.sort_by(|a, b| {
            let ord = match self.port_sort_by {
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

    pub fn filtered_processes(&self) -> Vec<&ProcessInfo> {
        if self.filter_input.is_empty() {
            self.processes.iter().collect()
        } else {
            let filter = self.filter_input.to_lowercase();
            self.processes
                .iter()
                .filter(|p| {
                    p.name.to_lowercase().contains(&filter)
                        || p.command.to_lowercase().contains(&filter)
                        || p.pid.to_string().contains(&filter)
                })
                .collect()
        }
    }

    pub fn filtered_ports(&self) -> Vec<&PortInfo> {
        if self.filter_input.is_empty() {
            self.ports.iter().collect()
        } else {
            let filter = self.filter_input.to_lowercase();
            self.ports
                .iter()
                .filter(|p| {
                    p.local_port.to_string().contains(&filter)
                        || p.process_name.to_lowercase().contains(&filter)
                        || p.local_addr.contains(&filter)
                        || p.protocol.to_lowercase().contains(&filter)
                })
                .collect()
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match &self.input_mode {
            InputMode::ConfirmKill => self.handle_confirm_kill(key),
            InputMode::CommandPalette => self.handle_command_input(key),
            InputMode::Filter => self.handle_filter_input(key),
            InputMode::Normal => self.handle_normal(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => self.show_help = false,
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Esc => {
                if self.view != View::Dashboard {
                    self.view = View::Dashboard;
                    self.filter_input.clear();
                } else {
                    self.running = false;
                }
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char(':') => {
                self.input_mode = InputMode::CommandPalette;
                self.command_input.clear();
                self.command_error = None;
            }
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Filter;
                self.filter_input.clear();
            }

            // View switching by number
            KeyCode::Char('1') => self.view = View::Dashboard,
            KeyCode::Char('2') => self.view = View::CpuDetail,
            KeyCode::Char('3') => self.view = View::MemoryDetail,
            KeyCode::Char('4') => self.view = View::DiskDetail,
            KeyCode::Char('5') => self.view = View::GpuDetail,
            KeyCode::Char('6') => self.view = View::ProcessTable,
            KeyCode::Char('7') => self.view = View::PortTable,

            // Tab cycling
            KeyCode::Tab => {
                let idx = (self.view.index() + 1) % 7;
                self.view = View::from_index(idx);
                self.filter_input.clear();
            }
            KeyCode::BackTab => {
                let idx = if self.view.index() == 0 {
                    6
                } else {
                    self.view.index() - 1
                };
                self.view = View::from_index(idx);
                self.filter_input.clear();
            }

            // Table navigation
            KeyCode::Down | KeyCode::Char('j') => self.table_next(),
            KeyCode::Up | KeyCode::Char('k') => self.table_previous(),
            KeyCode::Home => self.table_first(),
            KeyCode::End => self.table_last(),
            KeyCode::PageDown => {
                for _ in 0..20 {
                    self.table_next();
                }
            }
            KeyCode::PageUp => {
                for _ in 0..20 {
                    self.table_previous();
                }
            }

            // Sort
            KeyCode::Char('s') => self.cycle_sort(false),
            KeyCode::Char('S') => self.cycle_sort(true),

            // Kill
            KeyCode::Delete | KeyCode::Char('K') => self.initiate_kill(),

            // Refresh rate
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let ms = self.tick_rate.as_millis() as u64;
                if ms > 250 {
                    self.tick_rate = Duration::from_millis(ms - 250);
                    self.set_status(format!("Refresh rate: {}ms", self.tick_rate.as_millis()));
                }
            }
            KeyCode::Char('-') => {
                let ms = self.tick_rate.as_millis() as u64;
                if ms < 10000 {
                    self.tick_rate = Duration::from_millis(ms + 250);
                    self.set_status(format!("Refresh rate: {}ms", self.tick_rate.as_millis()));
                }
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
                let cmd = self.command_input.clone();
                self.execute_command(&cmd);
                self.input_mode = InputMode::Normal;
                self.command_input.clear();
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
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
            }
            KeyCode::Char(c) => {
                self.filter_input.push(c);
            }
            _ => {}
        }
    }

    fn handle_confirm_kill(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some((pid, name)) = self.confirm_kill.take() {
                    match self.proc_collector.kill_process(pid) {
                        Ok(()) => self.set_status(format!("Killed {} (PID {})", name, pid)),
                        Err(e) => self.set_status(format!("Failed to kill {}: {}", name, e)),
                    }
                }
                self.input_mode = InputMode::Normal;
            }
            _ => {
                self.confirm_kill = None;
                self.input_mode = InputMode::Normal;
            }
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "q" | "quit" => self.running = false,
            "kill" => {
                if let Some(pid_str) = parts.get(1) {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        match self.proc_collector.kill_process(pid) {
                            Ok(()) => self.set_status(format!("Killed PID {}", pid)),
                            Err(e) => self.set_status(format!("Failed: {}", e)),
                        }
                    } else {
                        self.set_status("Invalid PID".to_string());
                    }
                } else {
                    self.set_status("Usage: kill <pid>".to_string());
                }
            }
            "kill-port" => {
                if let Some(port_str) = parts.get(1) {
                    if let Ok(port) = port_str.parse::<u16>() {
                        self.kill_by_port(port);
                    } else {
                        self.set_status("Invalid port number".to_string());
                    }
                } else {
                    self.set_status("Usage: kill-port <port>".to_string());
                }
            }
            "rate" => {
                if let Some(ms_str) = parts.get(1) {
                    if let Ok(ms) = ms_str.parse::<u64>() {
                        if ms >= 100 {
                            self.tick_rate = Duration::from_millis(ms);
                            self.set_status(format!("Refresh rate: {}ms", ms));
                        } else {
                            self.set_status("Minimum rate is 100ms".to_string());
                        }
                    } else {
                        self.set_status("Invalid rate value".to_string());
                    }
                } else {
                    self.set_status("Usage: rate <ms>".to_string());
                }
            }
            "filter" => {
                self.filter_input = parts[1..].join(" ");
                self.set_status(format!("Filter: {}", self.filter_input));
            }
            "sort" => {
                if let Some(col) = parts.get(1) {
                    match col.to_lowercase().as_str() {
                        "cpu" => self.process_sort_by = ProcessSortBy::Cpu,
                        "mem" | "memory" => self.process_sort_by = ProcessSortBy::Memory,
                        "pid" => self.process_sort_by = ProcessSortBy::Pid,
                        "name" => self.process_sort_by = ProcessSortBy::Name,
                        "port" => self.port_sort_by = PortSortBy::Port,
                        "protocol" => self.port_sort_by = PortSortBy::Protocol,
                        "state" => self.port_sort_by = PortSortBy::State,
                        _ => {
                            self.set_status(format!("Unknown sort column: {}", col));
                            return;
                        }
                    }
                    self.set_status(format!("Sorting by: {}", col));
                } else {
                    self.set_status("Usage: sort <column>".to_string());
                }
            }
            _ => {
                self.set_status(format!("Unknown command: {}", parts[0]));
            }
        }
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

    fn table_next(&mut self) {
        match self.view {
            View::ProcessTable => {
                let len = self.filtered_processes().len();
                if len == 0 {
                    return;
                }
                let i = self
                    .process_table_state
                    .selected()
                    .map(|i| (i + 1).min(len - 1))
                    .unwrap_or(0);
                self.process_table_state.select(Some(i));
            }
            View::PortTable => {
                let len = self.filtered_ports().len();
                if len == 0 {
                    return;
                }
                let i = self
                    .port_table_state
                    .selected()
                    .map(|i| (i + 1).min(len - 1))
                    .unwrap_or(0);
                self.port_table_state.select(Some(i));
            }
            _ => {}
        }
    }

    fn table_previous(&mut self) {
        match self.view {
            View::ProcessTable => {
                let i = self
                    .process_table_state
                    .selected()
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(0);
                self.process_table_state.select(Some(i));
            }
            View::PortTable => {
                let i = self
                    .port_table_state
                    .selected()
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(0);
                self.port_table_state.select(Some(i));
            }
            _ => {}
        }
    }

    fn table_first(&mut self) {
        match self.view {
            View::ProcessTable => self.process_table_state.select(Some(0)),
            View::PortTable => self.port_table_state.select(Some(0)),
            _ => {}
        }
    }

    fn table_last(&mut self) {
        match self.view {
            View::ProcessTable => {
                let len = self.filtered_processes().len();
                if len > 0 {
                    self.process_table_state.select(Some(len - 1));
                }
            }
            View::PortTable => {
                let len = self.filtered_ports().len();
                if len > 0 {
                    self.port_table_state.select(Some(len - 1));
                }
            }
            _ => {}
        }
    }

    fn cycle_sort(&mut self, reverse: bool) {
        match self.view {
            View::ProcessTable => {
                if reverse {
                    self.process_sort_asc = !self.process_sort_asc;
                } else {
                    self.process_sort_by = match self.process_sort_by {
                        ProcessSortBy::Cpu => ProcessSortBy::Memory,
                        ProcessSortBy::Memory => ProcessSortBy::Pid,
                        ProcessSortBy::Pid => ProcessSortBy::Name,
                        ProcessSortBy::Name => ProcessSortBy::Cpu,
                    };
                }
                self.set_status(format!(
                    "Sort: {:?} ({})",
                    self.process_sort_by,
                    if self.process_sort_asc { "asc" } else { "desc" }
                ));
            }
            View::PortTable => {
                if reverse {
                    self.port_sort_asc = !self.port_sort_asc;
                } else {
                    self.port_sort_by = match self.port_sort_by {
                        PortSortBy::Port => PortSortBy::Protocol,
                        PortSortBy::Protocol => PortSortBy::Pid,
                        PortSortBy::Pid => PortSortBy::State,
                        PortSortBy::State => PortSortBy::Port,
                    };
                }
            }
            _ => {}
        }
    }

    fn initiate_kill(&mut self) {
        match self.view {
            View::ProcessTable => {
                if let Some(idx) = self.process_table_state.selected() {
                    let filtered = self.filtered_processes();
                    if let Some(proc) = filtered.get(idx) {
                        self.confirm_kill = Some((proc.pid, proc.name.clone()));
                        self.input_mode = InputMode::ConfirmKill;
                    }
                }
            }
            View::PortTable => {
                if let Some(idx) = self.port_table_state.selected() {
                    let filtered = self.filtered_ports();
                    if let Some(port) = filtered.get(idx) {
                        if let Some(pid) = port.pid {
                            self.confirm_kill = Some((pid, format!("port:{}", port.local_port)));
                            self.input_mode = InputMode::ConfirmKill;
                        } else {
                            self.set_status("No PID associated with this port".to_string());
                        }
                    }
                }
            }
            _ => {}
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
