use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row, Table},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::types::{format_bytes, ProcessSortBy};

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let procs = &app.data.processes;
    let count = app.process_view.order.len();

    let arrow_up = " \u{25b2}";
    let arrow_down = " \u{25bc}";

    let make_header = |label: &str, active: bool| -> Cell<'static> {
        if active {
            let arrow = if app.process_view.sort_asc {
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
        make_header("PID", app.process_view.sort_by == ProcessSortBy::Pid),
        Cell::from(Span::styled("User", theme::header_style())),
        make_header("Name", app.process_view.sort_by == ProcessSortBy::Name),
        make_header("CPU%", app.process_view.sort_by == ProcessSortBy::Cpu),
        make_header("Memory", app.process_view.sort_by == ProcessSortBy::Memory),
        Cell::from(Span::styled("Disk I/O", theme::header_style())),
        Cell::from(Span::styled("Status", theme::header_style())),
        Cell::from(Span::styled("Command", theme::header_style())),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .process_view
        .order
        .iter()
        .map(|&i| &procs[i])
        .enumerate()
        .map(|(i, p)| {
            let row = Row::new(vec![
                Cell::from(Span::styled(p.pid().to_string(), theme::text_style())),
                Cell::from(Span::styled(&*p.user, theme::dim_style())),
                Cell::from(Span::styled(&*p.name, theme::text_style())),
                Cell::from(Span::styled(
                    format!("{:.1}%", p.cpu_usage),
                    theme::text_style(),
                )),
                Cell::from(Span::styled(format_bytes(p.memory), theme::text_style())),
                Cell::from(Span::styled(
                    format!("{}/s", format_bytes(p.disk_rate)),
                    theme::dim_style(),
                )),
                Cell::from(Span::styled(p.status, theme::dim_style())),
                Cell::from(Span::styled(&*p.command, theme::dim_style())),
            ]);
            if i % 2 == 0 {
                row
            } else {
                row.style(Style::new().bg(theme::BG_ALT_ROW))
            }
        })
        .collect();

    let title = Line::from(vec![
        Span::styled(" Processes ", theme::title_style()),
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
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(22),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(theme::selected_style());

    frame.render_stateful_widget(table, area, &mut app.process_view.table_state);
}
