mod command_palette;
mod cpu_detail;
mod dashboard;
mod disk_detail;
mod gpu_detail;
mod help;
mod inspector;
mod memory_detail;
mod port_table;
mod process_table;
mod table;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, InputMode, View};
use crate::theme;

pub fn render(frame: &mut Frame, app: &mut App) {
    let [tab_area, content_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // Tab bar
    let tab_titles = View::titles();
    let tabs = Tabs::new(tab_titles.to_vec())
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" pulsemon ", theme::title_style())),
        )
        .select(app.view.index())
        .style(theme::dim_style())
        .highlight_style(theme::header_style())
        .divider(Span::styled(" | ", Style::new().fg(theme::TEXT_MUTED)));
    frame.render_widget(tabs, tab_area);

    if app.inspector.is_some() {
        inspector::render(frame, app, content_area);
    } else {
        render_view(frame, app, content_area);
    }

    // Status bar
    let status_line = if let Some(msg) = app.status_text() {
        Line::from(Span::styled(
            format!(" {} ", msg),
            Style::new().fg(theme::ORANGE),
        ))
    } else {
        let rate = app.tick_rate.as_millis();
        Line::from(vec![
            Span::styled(" q", Style::new().fg(theme::ORANGE)),
            Span::styled(":quit ", theme::dim_style()),
            Span::styled("?", Style::new().fg(theme::ORANGE)),
            Span::styled(":help ", theme::dim_style()),
            Span::styled("/", Style::new().fg(theme::ORANGE)),
            Span::styled(":filter ", theme::dim_style()),
            Span::styled("s", Style::new().fg(theme::ORANGE)),
            Span::styled(":sort ", theme::dim_style()),
            Span::styled("K", Style::new().fg(theme::ORANGE)),
            Span::styled(":kill ", theme::dim_style()),
            Span::styled(":", Style::new().fg(theme::ORANGE)),
            Span::styled(":cmd ", theme::dim_style()),
            Span::styled("+/-", Style::new().fg(theme::ORANGE)),
            Span::styled(format!(":rate({}ms) ", rate), theme::dim_style()),
            Span::styled("Tab", Style::new().fg(theme::ORANGE)),
            Span::styled(":view ", theme::dim_style()),
            Span::styled("1-7", Style::new().fg(theme::ORANGE)),
            Span::styled(":jump", theme::dim_style()),
        ])
    };
    let mut status_line = status_line;
    if let Some(cost) = app.tick_cost {
        status_line.push_span(Span::styled(
            format!(" collect {:.1}ms", cost.as_secs_f64() * 1000.0),
            Style::new().fg(theme::ORANGE_BRIGHT),
        ));
    }
    let status = Paragraph::new(status_line).style(Style::new().bg(theme::BG_HIGHLIGHT));
    frame.render_widget(status, status_area);

    // Overlays
    match &app.input_mode {
        InputMode::CommandPalette => command_palette::render(frame, app),
        InputMode::Filter => render_filter_bar(frame, app),
        InputMode::ConfirmKill => render_kill_confirm(frame, app),
        InputMode::Normal => {}
    }

    if app.show_help {
        help::render(frame);
    }
}

fn render_view(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.view {
        View::Dashboard => dashboard::render(frame, app, area),
        View::CpuDetail => cpu_detail::render(frame, app, area),
        View::MemoryDetail => memory_detail::render(frame, app, area),
        View::DiskDetail => disk_detail::render(frame, app, area),
        View::GpuDetail => gpu_detail::render(frame, app, area),
        View::ProcessTable => {
            process_table::render(frame, app, area);
        }
        View::PortTable => {
            port_table::render(frame, app, area);
        }
    }
}

fn render_filter_bar(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let filter_area = Rect {
        x: 0,
        y: area.height.saturating_sub(3),
        width: area.width,
        height: 3,
    };

    frame.render_widget(Clear, filter_area);

    let title = Line::from(vec![
        Span::styled(" Filter ", theme::title_style()),
        Span::styled("(Esc to cancel) ", theme::dim_style()),
    ]);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::active_border_style())
        .title(title);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("/", theme::dim_style()),
        Span::styled(app.active_filter(), Style::new().fg(theme::TEXT)),
    ]))
    .block(block);

    frame.render_widget(input, filter_area);
}

fn render_kill_confirm(frame: &mut Frame, app: &App) {
    if let Some((pid, name)) = &app.confirm_kill {
        let area = frame.area();
        let popup_width = 50u16.min(area.width);
        let popup_height = 5u16.min(area.height);
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup_area);

        let title = Line::from(Span::styled(
            " Confirm Kill ",
            Style::new()
                .fg(theme::RED)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(theme::RED))
            .title(title);

        let text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("  Kill {} ", name), Style::new().fg(theme::TEXT)),
                Span::styled(format!("(PID {})", pid), theme::dim_style()),
                Span::styled("  ?", Style::new().fg(theme::TEXT)),
            ]),
            Line::from(vec![
                Span::styled("  [", theme::dim_style()),
                Span::styled("y", Style::new().fg(theme::RED)),
                Span::styled("]es  /  [", theme::dim_style()),
                Span::styled("n", Style::new().fg(theme::GREEN)),
                Span::styled("]o", theme::dim_style()),
            ]),
        ];

        let popup = Paragraph::new(text).block(block);
        frame.render_widget(popup, popup_area);
    }
}

fn filter_title(filter: &str) -> Line<'_> {
    if filter.is_empty() {
        return Line::default();
    }
    Line::from(vec![
        Span::styled(" filter: ", theme::dim_style()),
        Span::styled(filter, Style::new().fg(theme::ORANGE)),
        Span::raw(" "),
    ])
}
