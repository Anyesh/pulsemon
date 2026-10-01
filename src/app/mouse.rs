use std::time::{Duration, Instant};

use crossterm::event::MouseButton;

use super::action::Action;
use crate::types::ProcKey;
use crate::ui::hit::Target;
use crate::views::port_view::PortKey;

pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const WHEEL_STEP: i32 = 3;

/// What a click landed on, for double-click detection. Rows are identified by the
/// process or socket they show rather than by screen position, because a refresh
/// can re-sort the table between the two clicks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickId {
    Process(ProcKey),
    Port(PortKey),
    Other(Target),
}

#[derive(Debug, Default)]
pub struct ClickTracker {
    last: Option<(ClickId, Instant)>,
}

impl ClickTracker {
    /// Records a left press; true when it completes a double click on the same thing.
    pub fn press(&mut self, id: ClickId, at: Instant) -> bool {
        let double = matches!(
            &self.last,
            Some((last, when)) if *last == id && at.duration_since(*when) <= DOUBLE_CLICK
        );
        self.last = if double { None } else { Some((id, at)) };
        double
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

pub fn click_action(target: &Target, button: MouseButton, double: bool) -> Option<Action> {
    match button {
        MouseButton::Left => left_click(target, double),
        MouseButton::Right => match target {
            Target::ProcessRow(_)
            | Target::TopProcess(_)
            | Target::PortRow(_)
            | Target::InspectorLink(_) => Some(Action::ContextMenu(Box::new(target.clone()))),
            _ => None,
        },
        MouseButton::Middle => None,
    }
}

fn left_click(target: &Target, double: bool) -> Option<Action> {
    Some(match target {
        Target::Tab(view) | Target::Panel(view) => Action::SwitchView(*view),
        Target::ProcessHeader(col) => Action::ClickProcessColumn(*col),
        Target::PortHeader(col) => Action::ClickPortColumn(*col),
        Target::ProcessRow(key) | Target::TopProcess(key) if double => Action::Inspect(*key),
        Target::ProcessRow(key) | Target::TopProcess(key) => Action::SelectProcess(*key),
        Target::PortRow(pos) if double => Action::InspectPortAt(*pos),
        Target::PortRow(pos) => Action::SelectPortAt(*pos),
        Target::InspectorLink(key) => Action::Inspect(*key),
        Target::Modal(action) => return action.clone(),
        Target::Button(action) => action.clone(),
        Target::TableBody(_) | Target::InspectorBody => return None,
    })
}

pub fn scroll_action(target: &Target, up: bool) -> Option<Action> {
    let step = if up { -WHEEL_STEP } else { WHEEL_STEP };
    match target {
        Target::TableBody(kind) => Some(Action::ScrollTable(*kind, step)),
        Target::InspectorBody => Some(Action::ScrollInspector(step)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::View;
    use crate::views::process_view::ProcessColumn;
    use crate::views::TableKind;

    const KEY: ProcKey = ProcKey {
        pid: 42,
        start_time: 1000,
    };

    #[test]
    fn second_press_on_same_process_within_window_is_double() {
        let t0 = Instant::now();
        let mut clicks = ClickTracker::default();
        assert!(!clicks.press(ClickId::Process(KEY), t0));
        assert!(clicks.press(ClickId::Process(KEY), t0 + Duration::from_millis(250)));
        assert!(
            !clicks.press(ClickId::Process(KEY), t0 + Duration::from_millis(300)),
            "a third press starts over"
        );
    }

    #[test]
    fn slow_or_different_second_press_is_single() {
        let t0 = Instant::now();
        let mut clicks = ClickTracker::default();
        clicks.press(ClickId::Process(KEY), t0);
        assert!(!clicks.press(
            ClickId::Process(KEY),
            t0 + DOUBLE_CLICK + Duration::from_millis(1)
        ));
        let other = ProcKey { pid: 43, ..KEY };
        assert!(!clicks.press(
            ClickId::Process(other),
            t0 + DOUBLE_CLICK + Duration::from_millis(50)
        ));
    }

    #[test]
    fn same_pid_with_new_start_time_is_a_different_process() {
        let t0 = Instant::now();
        let mut clicks = ClickTracker::default();
        clicks.press(ClickId::Process(KEY), t0);
        let reborn = ProcKey {
            start_time: 2000,
            ..KEY
        };
        assert!(!clicks.press(ClickId::Process(reborn), t0 + Duration::from_millis(100)));
    }

    #[test]
    fn rows_select_on_click_and_inspect_on_double_click() {
        let row = Target::ProcessRow(KEY);
        assert_eq!(
            click_action(&row, MouseButton::Left, false),
            Some(Action::SelectProcess(KEY))
        );
        assert_eq!(
            click_action(&row, MouseButton::Left, true),
            Some(Action::Inspect(KEY))
        );
        assert_eq!(
            click_action(&Target::PortRow(3), MouseButton::Left, true),
            Some(Action::InspectPortAt(3))
        );
    }

    #[test]
    fn right_click_on_a_row_opens_its_menu() {
        let row = Target::ProcessRow(KEY);
        assert_eq!(
            click_action(&row, MouseButton::Right, false),
            Some(Action::ContextMenu(Box::new(row.clone())))
        );
        assert_eq!(
            click_action(&Target::Tab(View::Dashboard), MouseButton::Right, false),
            None
        );
    }

    #[test]
    fn tabs_headers_panels_and_links() {
        assert_eq!(
            click_action(&Target::Tab(View::PortTable), MouseButton::Left, false),
            Some(Action::SwitchView(View::PortTable))
        );
        assert_eq!(
            click_action(
                &Target::ProcessHeader(ProcessColumn::Memory),
                MouseButton::Left,
                true
            ),
            Some(Action::ClickProcessColumn(ProcessColumn::Memory)),
            "a fast second header click still just flips the direction"
        );
        assert_eq!(
            click_action(&Target::Panel(View::CpuDetail), MouseButton::Left, false),
            Some(Action::SwitchView(View::CpuDetail))
        );
        assert_eq!(
            click_action(&Target::InspectorLink(KEY), MouseButton::Left, false),
            Some(Action::Inspect(KEY))
        );
        assert_eq!(
            click_action(&Target::Modal(None), MouseButton::Left, false),
            None
        );
        assert_eq!(
            click_action(&Target::Button(Action::Quit), MouseButton::Left, false),
            Some(Action::Quit)
        );
    }

    #[test]
    fn wheel_scrolls_tables_and_inspector() {
        assert_eq!(
            scroll_action(&Target::TableBody(TableKind::Ports), false),
            Some(Action::ScrollTable(TableKind::Ports, WHEEL_STEP))
        );
        assert_eq!(
            scroll_action(&Target::InspectorBody, true),
            Some(Action::ScrollInspector(-WHEEL_STEP))
        );
        assert_eq!(scroll_action(&Target::Tab(View::Dashboard), true), None);
    }
}
