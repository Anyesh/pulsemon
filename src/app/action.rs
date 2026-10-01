use super::View;
use crate::views::port_view::PortColumn;
use crate::views::process_view::ProcessColumn;

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
}
