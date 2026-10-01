use ratatui::{
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row},
    Frame,
};

use super::table::{self, Column};
use crate::app::App;
use crate::theme;
use crate::types::format_bytes;
use crate::views::process_view::ProcessColumn;

pub const COLUMNS: [Column<ProcessColumn>; 8] = [
    Column {
        label: "PID",
        width: Constraint::Length(7),
        key: ProcessColumn::Pid,
    },
    Column {
        label: "User",
        width: Constraint::Length(10),
        key: ProcessColumn::User,
    },
    Column {
        label: "Name",
        width: Constraint::Length(20),
        key: ProcessColumn::Name,
    },
    Column {
        label: "CPU%",
        width: Constraint::Length(7),
        key: ProcessColumn::Cpu,
    },
    Column {
        label: "Memory",
        width: Constraint::Length(10),
        key: ProcessColumn::Memory,
    },
    Column {
        label: "Disk I/O",
        width: Constraint::Length(11),
        key: ProcessColumn::Disk,
    },
    Column {
        label: "Status",
        width: Constraint::Length(9),
        key: ProcessColumn::Status,
    },
    Column {
        label: "Command",
        width: Constraint::Fill(1),
        key: ProcessColumn::Command,
    },
];

/// Returns the table's inner area.
pub fn render(frame: &mut Frame, app: &mut App, area: Rect) -> Rect {
    let view = &mut app.process_view;
    let procs = &app.data.processes;
    let count = view.order.len();

    let title = Line::from(vec![
        Span::styled(" Processes ", theme::title_style()),
        Span::styled(format!("({}) ", count), theme::dim_style()),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(title)
        .title_bottom(super::filter_title(&view.filter));

    let order = &view.order;
    table::render(
        frame,
        area,
        block,
        &COLUMNS,
        view.sort,
        &mut view.cursor,
        count,
        view.selected.is_some(),
        |pos| {
            let p = &procs[order[pos]];
            Row::new([
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
            ])
        },
    )
}
