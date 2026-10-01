use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row, Table},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::types::PortSortBy;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let filtered = app.filtered_ports();
    let count = filtered.len();

    let arrow_up = " \u{25b2}";
    let arrow_down = " \u{25bc}";

    let make_header = |label: &str, active: bool| -> Cell<'static> {
        if active {
            let arrow = if app.port_sort_asc {
                arrow_up
            } else {
                arrow_down
            };
            Cell::from(Line::from(vec![
                Span::styled(label.to_string(), theme::header_style()),
                Span::styled(arrow.to_string(), Style::new().fg(theme::ORANGE_BRIGHT)),
            ]))
        } else {
            Cell::from(Span::styled(label.to_string(), theme::header_style()))
        }
    };

    let header = Row::new(vec![
        make_header("Protocol", app.port_sort_by == PortSortBy::Protocol),
        Cell::from(Span::styled("Local Address", theme::header_style())),
        make_header("Port", app.port_sort_by == PortSortBy::Port),
        Cell::from(Span::styled("Remote Address", theme::header_style())),
        make_header("State", app.port_sort_by == PortSortBy::State),
        make_header("PID", app.port_sort_by == PortSortBy::Pid),
        Cell::from(Span::styled("Process", theme::header_style())),
    ])
    .height(1);

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let row = Row::new(vec![
                Cell::from(Span::styled(p.protocol.clone(), theme::text_style())),
                Cell::from(Span::styled(p.local_addr.clone(), theme::text_style())),
                Cell::from(Span::styled(p.local_port.to_string(), theme::text_style())),
                Cell::from(Span::styled(p.remote_addr.clone(), theme::dim_style())),
                Cell::from(Span::styled(p.state.clone(), theme::text_style())),
                Cell::from(Span::styled(
                    p.pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "-".into()),
                    theme::dim_style(),
                )),
                Cell::from(Span::styled(p.process_name.clone(), theme::text_style())),
            ]);
            if i % 2 == 0 {
                row
            } else {
                row.style(Style::new().bg(theme::BG_ALT_ROW))
            }
        })
        .collect();

    let title = Line::from(vec![
        Span::styled(" Ports ", theme::title_style()),
        Span::styled(format!("({}) ", count), theme::dim_style()),
    ]);

    let bottom_title = if !app.filter_input.is_empty() {
        Line::from(vec![
            Span::styled(" filter: ", theme::dim_style()),
            Span::styled(
                format!("{} ", app.filter_input),
                Style::new().fg(theme::ORANGE),
            ),
        ])
    } else {
        Line::default()
    };

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(title)
        .title_bottom(bottom_title);

    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Length(18),
            Constraint::Length(7),
            Constraint::Length(18),
            Constraint::Length(13),
            Constraint::Length(8),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected_style());

    frame.render_stateful_widget(table, area, &mut app.port_table_state);
}
