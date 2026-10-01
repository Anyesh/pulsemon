use ratatui::{
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row},
    Frame,
};

use super::table::{self, Column};
use crate::app::App;
use crate::theme;
use crate::views::port_view::PortColumn;

pub const COLUMNS: [Column<PortColumn>; 7] = [
    Column {
        label: "Proto",
        width: Constraint::Length(7),
        key: PortColumn::Protocol,
    },
    Column {
        label: "Local Address",
        width: Constraint::Fill(2),
        key: PortColumn::Local,
    },
    Column {
        label: "Port",
        width: Constraint::Length(7),
        key: PortColumn::Port,
    },
    Column {
        label: "Remote",
        width: Constraint::Fill(2),
        key: PortColumn::Remote,
    },
    Column {
        label: "State",
        width: Constraint::Length(12),
        key: PortColumn::State,
    },
    Column {
        label: "PID",
        width: Constraint::Length(8),
        key: PortColumn::Pid,
    },
    Column {
        label: "Process",
        width: Constraint::Fill(1),
        key: PortColumn::Process,
    },
];

/// Returns the table's inner area.
pub fn render(frame: &mut Frame, app: &mut App, area: Rect) -> Rect {
    let view = &mut app.port_view;
    let ports = &app.ports;
    let count = view.order.len();

    let title = Line::from(vec![
        Span::styled(" Ports ", theme::title_style()),
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
            let p = &ports[order[pos]];
            let remote = if p.remote_port == 0 {
                p.remote_addr.clone()
            } else {
                format!("{}:{}", p.remote_addr, p.remote_port)
            };
            Row::new([
                Cell::from(Span::styled(p.protocol.as_str(), theme::text_style())),
                Cell::from(Span::styled(p.local_addr.as_str(), theme::text_style())),
                Cell::from(Span::styled(p.local_port.to_string(), theme::text_style())),
                Cell::from(Span::styled(remote, theme::dim_style())),
                Cell::from(Span::styled(p.state.as_str(), theme::text_style())),
                Cell::from(Span::styled(
                    p.pid.map_or_else(|| "-".to_string(), |pid| pid.to_string()),
                    theme::dim_style(),
                )),
                Cell::from(Span::styled(p.process_name.as_str(), theme::text_style())),
            ])
        },
    )
}
