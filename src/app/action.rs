use super::View;
use crate::types::{PortSortBy, ProcessSortBy};

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
    SortProcesses(ProcessSortBy),
    SortPorts(PortSortBy),
    RequestKill,
    Kill(u32),
    KillPort(u16),
    AdjustRate(i64),
    SetRate(u64),
    SetFilter(String),
}
