use std::rc::Rc;

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
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

/// Header cell areas for a table drawn inside `inner` (the block's inner area).
/// This mirrors how ratatui's `Table` lays out columns: a selection gutter of width
/// zero (we never draw a highlight symbol), then `Flex::Start` with spacing 1.
pub fn column_rects<K>(inner: Rect, columns: &[Column<K>]) -> Rc<[Rect]> {
    let header = Rect { height: 1, ..inner };
    let [_gutter, columns_area] =
        Layout::horizontal([Constraint::Length(0), Constraint::Fill(0)]).areas(header);
    Layout::horizontal(columns.iter().map(|c| c.width))
        .flex(Flex::Start)
        .spacing(1)
        .split(columns_area)
}

/// Screen row of each visible table row, paired with its position in the full list.
pub fn visible_rows(
    inner: Rect,
    cursor: &TableCursor,
    len: usize,
) -> impl Iterator<Item = (Rect, usize)> {
    let body = body_rect(inner);
    let end = (cursor.offset + cursor.viewport).min(len);
    (cursor.offset..end)
        .zip(body.y..body.bottom())
        .map(move |(pos, y)| {
            (
                Rect {
                    y,
                    height: 1,
                    ..body
                },
                pos,
            )
        })
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
    use ratatui::{backend::TestBackend, widgets::Borders, Terminal};

    use super::*;

    const COLUMNS: [Column<u8>; 4] = [
        Column {
            label: "PID",
            width: Constraint::Length(7),
            key: 0,
        },
        Column {
            label: "Name",
            width: Constraint::Length(12),
            key: 1,
        },
        Column {
            label: "CPU%",
            width: Constraint::Length(8),
            key: 2,
        },
        Column {
            label: "Command",
            width: Constraint::Fill(1),
            key: 3,
        },
    ];

    fn rendered_header_positions(width: u16, selected: bool) -> (Vec<u16>, Vec<Rect>) {
        let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
        let mut inner = Rect::default();
        terminal
            .draw(|frame| {
                let mut cursor = TableCursor::default();
                inner = render(
                    frame,
                    frame.area(),
                    Block::default().borders(Borders::ALL),
                    &COLUMNS,
                    Sort {
                        column: 1,
                        ascending: true,
                    },
                    &mut cursor,
                    3,
                    selected,
                    |pos| Row::new(vec![Cell::from(pos.to_string()); 4]),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let cells: Vec<String> = (0..width)
            .map(|x| buffer[(x, inner.y)].symbol().to_string())
            .collect();
        let starts = COLUMNS
            .iter()
            .map(|c| {
                let label: Vec<String> = c.label.chars().map(String::from).collect();
                cells
                    .windows(label.len())
                    .position(|w| w == label.as_slice())
                    .expect("label drawn") as u16
            })
            .collect();
        (starts, column_rects(inner, &COLUMNS).to_vec())
    }

    #[test]
    fn header_rects_match_rendered_labels() {
        for width in [40, 80, 157] {
            for selected in [false, true] {
                let (starts, rects) = rendered_header_positions(width, selected);
                let xs: Vec<u16> = rects.iter().map(|r| r.x).collect();
                assert_eq!(starts, xs, "width {width}, selected {selected}");
            }
        }
    }

    #[test]
    fn body_starts_below_header() {
        let inner = Rect::new(1, 1, 50, 10);
        assert_eq!(body_rect(inner), Rect::new(1, 2, 50, 9));
    }
}
