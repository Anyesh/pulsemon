use super::View;
use crate::types::ProcKey;
use crate::ui::hit::Target;
use crate::views::port_view::PortColumn;
use crate::views::process_view::ProcessColumn;
use crate::views::TableKind;

/// Every user intent, whatever its source (key, mouse or command palette), resolves to
/// one of these so each behaviour has a single implementation in `App::apply`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    Back,
    ToggleHelp,
    OpenPalette,
    OpenFilter,
    SwitchView(View),
    CycleView(i8),
    MoveSelection(i32),
    SelectFirst,
    SelectLast,
    CycleSort,
    ToggleSortDir,
    SortProcesses(ProcessColumn),
    SortPorts(PortColumn),
    RequestKill,
    Kill(u32),
    KillPort(u16),
    AdjustRate(i64),
    SetRate(u64),
    SetFilter(String),
    /// Inspect the selected process (or the owner of the selected port).
    OpenInspector,
    InspectPid(u32),
    CloseInspector,
    InspectorBack,
    /// Re-inspect the focused ancestor or child.
    InspectLink,
    ToggleEnv,
    ScrollInspector(i32),
    Inspect(ProcKey),
    SelectProcess(ProcKey),
    SelectPortAt(usize),
    InspectPortAt(usize),
    ClickProcessColumn(ProcessColumn),
    ClickPortColumn(PortColumn),
    ScrollTable(TableKind, i32),
    /// Right click: act on the process behind whatever was clicked.
    ContextMenu(Box<Target>),
    ConfirmKill(bool),
    ToggleMouse,
}
