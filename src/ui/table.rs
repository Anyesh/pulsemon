use ratatui::{
    layout::{Constraint, Flex, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

use crate::theme;
use crate::views::{Sort, TableCursor};

/// One table column. The same descriptor drives rendering and header hit-testing,
/// so the clickable areas cannot drift from what is drawn.
pub struct Column<K> {
    pub label: &'static str,
    pub width: Constraint,
    pub key: K,
}

/// Row area under the header, one terminal row per table row.
pub fn body_rect(inner: Rect) -> Rect {
    Rect {
        y: inner.y.saturating_add(1),
        height: inner.height.saturating_sub(1),
        ..inner
    }
}

fn header_row<K: Copy + PartialEq>(columns: &[Column<K>], sort: Sort<K>) -> Row<'static> {
    let cells = columns.iter().map(|col| {
        let label = Span::styled(col.label, theme::header_style());
        if col.key == sort.column {
            let arrow = if sort.ascending {
                " \u{25b2}"
            } else {
                " \u{25bc}"
            };
            Cell::from(Line::from(vec![
                label,
                Span::styled(arrow, Style::new().fg(theme::ORANGE_BRIGHT)),
            ]))
        } else {
            Cell::from(label)
        }
    });
    Row::new(cells).height(1)
}

/// Renders only the visible slice of a table whose scroll window we own.
/// `row_at(pos)` builds the row at position `pos` of the full (filtered, sorted) list.
/// Returns the block's inner area for hit-testing.
#[allow(clippy::too_many_arguments)]
pub fn render<'a, K: Copy + PartialEq>(
    frame: &mut Frame,
    area: Rect,
    block: Block<'a>,
    columns: &[Column<K>],
    sort: Sort<K>,
    cursor: &mut TableCursor,
    len: usize,
    has_selection: bool,
    row_at: impl Fn(usize) -> Row<'a>,
) -> Rect {
    let inner = block.inner(area);
    let body = body_rect(inner);
    cursor.fit(len, body.height as usize);

    let end = (cursor.offset + cursor.viewport).min(len);
    let rows = (cursor.offset..end).map(|pos| {
        let row = row_at(pos);
        if pos % 2 == 0 {
            row
        } else {
            row.style(Style::new().bg(theme::BG_ALT_ROW))
        }
    });

    let mut state = TableState::default()
        .with_selected(has_selection.then(|| cursor.visible_cursor()).flatten());
    let table = Table::new(rows, columns.iter().map(|c| c.width))
        .header(header_row(columns, sort))
        .block(block)
        .row_highlight_style(theme::selected_style())
        .highlight_spacing(HighlightSpacing::Never)
        .flex(Flex::Start)
        .column_spacing(1);
    frame.render_stateful_widget(table, area, &mut state);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_starts_below_header() {
        let inner = Rect::new(1, 1, 50, 10);
        assert_eq!(body_rect(inner), Rect::new(1, 2, 50, 9));
    }
}
