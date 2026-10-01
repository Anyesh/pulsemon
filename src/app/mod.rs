pub mod action;
mod command;
mod keymap;
mod mouse;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};

use crate::collectors::inspect::ProcessDetail;
use crate::collectors::worker::{CollectorHandle, Request, Snapshot};
use crate::config::Config;
use crate::types::*;
use crate::ui::hit::{HitMap, Target};
use crate::views::inspector::InspectorView;
use crate::views::port_view::PortView;
use crate::views::process_view::ProcessView;
use crate::views::TableKind;
use action::Action;
use mouse::{ClickId, ClickTracker};

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
    /// When set, the inspector replaces the current view's content.
    pub inspector: Option<InspectorView>,
    /// Clickable regions of the last frame, filled in by the renderers.
    pub hits: HitMap,
    clicks: ClickTracker,
    /// Whether the terminal should report mouse events; the main loop applies it.
    pub mouse_capture: bool,

    pub command_input: String,
    pub command_error: Option<String>,
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
            inspector: None,
            hits: HitMap::default(),
            clicks: ClickTracker::default(),
            mouse_capture: !config.no_mouse,
            command_input: String::new(),
            command_error: None,
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
        let detail = snapshot.detail.take();
        self.data = snapshot;
        self.rebuild_views();
        if let Some(inspector) = &mut self.inspector {
            inspector.update(&self.data.processes);
        }
        if let Some(detail) = detail {
            self.apply_detail(*detail);
        }
    }

    pub fn apply_detail(&mut self, detail: ProcessDetail) {
        if let Some(inspector) = &mut self.inspector {
            inspector.set_detail(detail);
        }
    }

    fn rebuild_views(&mut self) {
        self.process_view.rebuild(&self.data.processes);
        self.port_view.rebuild(&self.ports);
    }

    /// The table that table-level actions (move, sort, filter) apply to.
    fn table(&self) -> Option<TableKind> {
        match self.view {
            View::ProcessTable => Some(TableKind::Processes),
            View::PortTable => Some(TableKind::Ports),
            _ => None,
        }
    }

    /// Filtering needs a table; from views without one it targets processes.
    fn filter_table(&self) -> TableKind {
        self.table().unwrap_or(TableKind::Processes)
    }

    pub fn active_filter(&self) -> &str {
        match self.filter_table() {
            TableKind::Processes => &self.process_view.filter,
            TableKind::Ports => &self.port_view.filter,
        }
    }

    fn active_filter_mut(&mut self) -> &mut String {
        match self.filter_table() {
            TableKind::Processes => &mut self.process_view.filter,
            TableKind::Ports => &mut self.port_view.filter,
        }
    }

    pub fn selected_process(&self) -> Option<&ProcessInfo> {
        let view = &self.process_view;
        let key = view.selected?;
        let row = &self.data.processes[*view.order.get(view.cursor.cursor)?];
        (row.key == key).then_some(row)
    }

    pub fn selected_port(&self) -> Option<&PortInfo> {
        self.port_view.selected(&self.ports)
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
                let action = if self.inspector.is_some() {
                    keymap::inspector(key)
                } else {
                    keymap::normal(key)
                };
                if let Some(action) = action {
                    self.apply(action);
                }
            }
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, at: Instant) {
        let (column, row) = (event.column, event.row);
        match event.kind {
            MouseEventKind::Down(button) => {
                let Some(target) = self.hits.hit(column, row).cloned() else {
                    self.clicks.reset();
                    return;
                };
                let double =
                    button == MouseButton::Left && self.clicks.press(self.click_id(&target), at);
                if let Some(action) = mouse::click_action(&target, button, double) {
                    self.apply(action);
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = event.kind == MouseEventKind::ScrollUp;
                let action = self
                    .hits
                    .scroll_target(column, row)
                    .and_then(|target| mouse::scroll_action(target, up));
                if let Some(action) = action {
                    self.apply(action);
                }
            }
            _ => {}
        }
    }

    fn click_id(&self, target: &Target) -> ClickId {
        match target {
            Target::ProcessRow(key) | Target::TopProcess(key) => ClickId::Process(*key),
            Target::PortRow(pos) => match self.port_view.key_at(&self.ports, *pos) {
                Some(key) => ClickId::Port(key),
                None => ClickId::Other(target.clone()),
            },
            other => ClickId::Other(other.clone()),
        }
    }

    /// The hit map describes the previous frame, which no longer matches the screen.
    pub fn on_resize(&mut self) {
        self.hits.clear();
        self.clicks.reset();
    }

    pub fn apply(&mut self, action: Action) {
        let resorted = sort_target(&action, self.table());
        match action {
            Action::Quit => self.running = false,
            Action::Back => {
                if self.table().is_some() && !self.active_filter().is_empty() {
                    self.active_filter_mut().clear();
                    self.rebuild_views();
                } else if self.view != View::Dashboard {
                    self.view = View::Dashboard;
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
                self.close_inspector();
                if self.table().is_none() {
                    self.switch_view(View::ProcessTable);
                }
                self.input_mode = InputMode::Filter;
                self.active_filter_mut().clear();
                self.rebuild_views();
            }
            Action::SwitchView(view) => self.switch_view(view),
            Action::CycleView(step) => {
                let idx = (self.view.index() + VIEW_COUNT).wrapping_add_signed(step as isize);
                self.switch_view(View::from_index(idx % VIEW_COUNT));
            }
            Action::MoveSelection(delta) => match &mut self.inspector {
                Some(inspector) => inspector.move_link(delta as i64),
                None => self.move_selection(delta as i64),
            },
            Action::SelectFirst => self.select_index(0),
            Action::SelectLast => self.select_index(usize::MAX),
            Action::CycleSort => match self.table() {
                Some(TableKind::Processes) => self.process_view.cycle_sort(),
                Some(TableKind::Ports) => self.port_view.cycle_sort(),
                None => return,
            },
            Action::ToggleSortDir => match self.table() {
                Some(TableKind::Processes) => {
                    self.process_view.sort.ascending = !self.process_view.sort.ascending
                }
                Some(TableKind::Ports) => {
                    self.port_view.sort.ascending = !self.port_view.sort.ascending
                }
                None => return,
            },
            Action::SortProcesses(col) => self.process_view.set_sort_column(col),
            Action::SortPorts(col) => self.port_view.set_sort_column(col),
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
                self.set_status(format!("Filter: {}", filter));
                *self.active_filter_mut() = filter;
                self.rebuild_views();
            }
            Action::OpenInspector => match self.selected_target() {
                Some(key) => self.inspect(key),
                None => self.set_status("Select a process to inspect".into()),
            },
            Action::InspectPid(pid) => match self.data.processes.iter().find(|p| p.pid() == pid) {
                Some(row) => self.inspect(row.key),
                None => self.set_status(format!("No process with PID {pid}")),
            },
            Action::CloseInspector => self.close_inspector(),
            Action::InspectorBack => {
                let rows = &self.data.processes;
                let went_back = match &mut self.inspector {
                    Some(inspector) => inspector.back(rows).then_some(inspector.key),
                    None => None,
                };
                match went_back {
                    Some(key) => self.collector.send(Request::Inspect(Some(key))),
                    None => self.close_inspector(),
                }
            }
            Action::InspectLink => {
                let link = self
                    .inspector
                    .as_ref()
                    .and_then(|i| i.link(i.link_cursor))
                    .map(|l| l.key);
                if let Some(key) = link {
                    self.inspect(key);
                }
            }
            Action::ToggleEnv => {
                if let Some(inspector) = &mut self.inspector {
                    inspector.show_env = !inspector.show_env;
                }
            }
            Action::ScrollInspector(delta) => {
                if let Some(inspector) = &mut self.inspector {
                    inspector.scroll = inspector.scroll.saturating_add_signed(delta as i16);
                }
            }
            Action::Inspect(key) => self.inspect(key),
            Action::SelectProcess(key) => self.process_view.select_key(&self.data.processes, key),
            Action::SelectPortAt(pos) => self.port_view.select_index(&self.ports, pos),
            Action::InspectPortAt(pos) => {
                self.port_view.select_index(&self.ports, pos);
                self.apply(Action::OpenInspector);
            }
            Action::ClickProcessColumn(col) => self.process_view.click_column(col),
            Action::ClickPortColumn(col) => self.port_view.click_column(col),
            Action::ScrollTable(kind, delta) => match kind {
                TableKind::Processes => {
                    let len = self.process_view.order.len();
                    self.process_view.cursor.scroll(len, delta as i64);
                }
                TableKind::Ports => {
                    let len = self.port_view.order.len();
                    self.port_view.cursor.scroll(len, delta as i64);
                }
            },
            Action::ContextMenu(target) => self.context_menu(&target),
            Action::ConfirmKill(confirmed) => self.finish_kill(confirmed),
            Action::ToggleMouse => {
                self.mouse_capture = !self.mouse_capture;
                self.set_status(if self.mouse_capture {
                    "Mouse on".into()
                } else {
                    "Mouse off: drag to select text, m to turn it back on".into()
                });
            }
        }
        if let Some(table) = resorted {
            self.rebuild_views();
            self.set_status(match table {
                TableKind::Processes => self.process_view.sort_label(),
                TableKind::Ports => self.port_view.sort_label(),
            });
        }
    }

    /// The process a table selection points at: the selected process, or the owner of
    /// the selected port.
    fn selected_target(&self) -> Option<ProcKey> {
        match self.table()? {
            TableKind::Processes => self.selected_process().map(|p| p.key),
            TableKind::Ports => {
                let pid = self.selected_port()?.pid?;
                self.data
                    .processes
                    .iter()
                    .find(|p| p.pid() == pid)
                    .map(|p| p.key)
            }
        }
    }

    fn inspect(&mut self, key: ProcKey) {
        let rows = &self.data.processes;
        match &mut self.inspector {
            Some(inspector) if inspector.key == key => return,
            Some(inspector) => inspector.follow(key, rows),
            None => self.inspector = Some(InspectorView::new(key, rows)),
        }
        self.collector.send(Request::Inspect(Some(key)));
    }

    fn close_inspector(&mut self) {
        if self.inspector.take().is_some() {
            self.collector.send(Request::Inspect(None));
        }
    }

    fn switch_view(&mut self, view: View) {
        self.close_inspector();
        let entering_ports = view == View::PortTable && self.view != View::PortTable;
        self.view = view;
        if entering_ports {
            // Ports are not scanned while hidden, so fetch them now instead of
            // showing a stale list until the next tick.
            self.collector.send(Request::Refresh { ports: true });
        }
    }

    fn select_index(&mut self, index: usize) {
        match self.table() {
            Some(TableKind::Processes) => {
                self.process_view.select_index(&self.data.processes, index)
            }
            Some(TableKind::Ports) => self.port_view.select_index(&self.ports, index),
            None => {}
        }
    }

    fn move_selection(&mut self, delta: i64) {
        match self.table() {
            Some(TableKind::Processes) => self.process_view.move_by(&self.data.processes, delta),
            Some(TableKind::Ports) => self.port_view.move_by(&self.ports, delta),
            None => {}
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
                match command::parse(&line, self.table() == Some(TableKind::Ports)) {
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
                self.active_filter_mut().clear();
            }
            KeyCode::Enter => self.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.active_filter_mut().pop();
            }
            KeyCode::Char(c) => self.active_filter_mut().push(c),
            _ => return,
        }
        self.rebuild_views();
    }

    fn handle_confirm_kill(&mut self, key: KeyEvent) {
        self.finish_kill(matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')));
    }

    fn finish_kill(&mut self, confirmed: bool) {
        if let Some((pid, label)) = self.confirm_kill.take() {
            if confirmed {
                self.collector.send(Request::Kill { pid, label });
            }
        }
        self.input_mode = InputMode::Normal;
    }

    fn context_menu(&mut self, target: &Target) {
        let key = match target {
            Target::ProcessRow(key) | Target::TopProcess(key) | Target::InspectorLink(key) => {
                Some(*key)
            }
            Target::PortRow(pos) => {
                self.port_view.select_index(&self.ports, *pos);
                self.selected_target()
            }
            _ => None,
        };
        let Some(row) = key.and_then(|k| self.data.processes.iter().find(|p| p.key == k)) else {
            return;
        };
        self.confirm_kill = Some((row.pid(), row.name.to_string()));
        self.input_mode = InputMode::ConfirmKill;
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
        if let Some(row) = self.inspector.as_ref().and_then(|i| i.row.as_ref()) {
            self.confirm_kill = Some((row.pid(), row.name.to_string()));
            self.input_mode = InputMode::ConfirmKill;
            return;
        }
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

/// Which table a sort action reorders, if any; those all need a rebuild and a status
/// line naming the new order.
fn sort_target(action: &Action, current: Option<TableKind>) -> Option<TableKind> {
    match action {
        Action::SortProcesses(_) => Some(TableKind::Processes),
        Action::SortPorts(_) => Some(TableKind::Ports),
        Action::ClickProcessColumn(_) => Some(TableKind::Processes),
        Action::ClickPortColumn(_) => Some(TableKind::Ports),
        Action::CycleSort | Action::ToggleSortDir => current,
        _ => None,
    }
}
