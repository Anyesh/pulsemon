use ratatui::layout::{Position, Rect};

use crate::app::action::Action;
use crate::app::View;
use crate::types::ProcKey;
use crate::views::port_view::PortColumn;
use crate::views::process_view::ProcessColumn;
use crate::views::TableKind;

/// Something on screen that responds to the mouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Tab(View),
    ProcessHeader(ProcessColumn),
    PortHeader(PortColumn),
    ProcessRow(ProcKey),
    /// Position in the ports table's current order.
    PortRow(usize),
    /// A table's scrollable area, for the wheel.
    TableBody(TableKind),
    /// A dashboard panel that opens its detail view.
    Panel(View),
    TopProcess(ProcKey),
    InspectorLink(ProcKey),
    InspectorBody,
    /// Covers the screen behind a modal so clicks cannot reach the view underneath;
    /// carries what a click outside the modal's own buttons should do.
    Modal(Option<Action>),
    Button(Action),
}

/// Clickable regions of the last frame. Renderers push regions as they draw, so the
/// map always describes what is on screen; later pushes sit on top.
#[derive(Debug, Default)]
pub struct HitMap {
    regions: Vec<(Rect, Target)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn push(&mut self, area: Rect, target: Target) {
        if !area.is_empty() {
            self.regions.push((area, target));
        }
    }

    pub fn hit(&self, column: u16, row: u16) -> Option<&Target> {
        let at = Position::new(column, row);
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains(at))
            .map(|(_, target)| target)
    }

    /// The scrollable container under the cursor, skipping rows and links drawn on
    /// top of it. A modal blocks scrolling of whatever is behind it.
    pub fn scroll_target(&self, column: u16, row: u16) -> Option<&Target> {
        let at = Position::new(column, row);
        self.regions
            .iter()
            .rev()
            .filter(|(area, _)| area.contains(at))
            .map(|(_, target)| target)
            .find(|t| {
                matches!(
                    t,
                    Target::TableBody(_) | Target::InspectorBody | Target::Modal(_)
                )
            })
            .filter(|t| !matches!(t, Target::Modal(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_pushed_region_wins() {
        let mut hits = HitMap::default();
        hits.push(
            Rect::new(0, 0, 10, 10),
            Target::TableBody(TableKind::Processes),
        );
        hits.push(Rect::new(0, 2, 10, 1), Target::InspectorBody);
        assert_eq!(hits.hit(3, 2), Some(&Target::InspectorBody));
        assert_eq!(
            hits.hit(3, 5),
            Some(&Target::TableBody(TableKind::Processes))
        );
        assert_eq!(hits.hit(30, 5), None);
    }

    #[test]
    fn modal_swallows_clicks_and_scrolls() {
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 10), Target::TableBody(TableKind::Ports));
        hits.push(Rect::new(0, 0, 80, 24), Target::Modal(None));
        hits.push(Rect::new(4, 4, 3, 1), Target::Button(Action::Quit));
        assert_eq!(hits.hit(1, 1), Some(&Target::Modal(None)));
        assert_eq!(hits.hit(5, 4), Some(&Target::Button(Action::Quit)));
        assert_eq!(hits.scroll_target(1, 1), None);
    }

    #[test]
    fn scroll_reaches_through_rows_to_their_table() {
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 10), Target::TableBody(TableKind::Ports));
        hits.push(Rect::new(0, 3, 10, 1), Target::PortRow(2));
        assert_eq!(
            hits.scroll_target(1, 3),
            Some(&Target::TableBody(TableKind::Ports))
        );
    }

    #[test]
    fn empty_areas_are_not_recorded() {
        let mut hits = HitMap::default();
        hits.push(Rect::new(5, 5, 0, 3), Target::InspectorBody);
        assert_eq!(hits.hit(5, 5), None);
    }
}
