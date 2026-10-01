pub mod port_view;
pub mod process_view;

use ratatui::widgets::TableState;

pub fn move_selection(state: &mut TableState, len: usize, delta: i32) {
    if len == 0 {
        return;
    }
    let next = match state.selected() {
        Some(i) => (i as i64 + delta as i64).clamp(0, len as i64 - 1) as usize,
        None => 0,
    };
    state.select(Some(next));
}

pub fn select_last(state: &mut TableState, len: usize) {
    if len > 0 {
        state.select(Some(len - 1));
    }
}
