mod command_palette;
mod cpu_detail;
mod dashboard;
mod disk_detail;
mod gpu_detail;
mod help;
pub mod hit;
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

use crate::app::action::Action;
use crate::app::{App, InputMode, View};
use crate::theme;
use hit::{HitMap, Target};

const TAB_DIVIDER: &str = " | ";

pub fn render(frame: &mut Frame, app: &mut App) {
    // Taken out for the frame so renderers can record targets while borrowing `app`.
    let mut hits = std::mem::take(&mut app.hits);
    hits.clear();
    render_frame(frame, app, &mut hits);
    app.hits = hits;
}

fn render_frame(frame: &mut Frame, app: &mut App, hits: &mut HitMap) {
    let [tab_area, content_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let tab_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" pulsemon ", theme::title_style()));
    let tab_inner = tab_block.inner(tab_area);
    let tabs = Tabs::new(View::titles().to_vec())
        .block(tab_block)
        .select(app.view.index())
        .style(theme::dim_style())
        .highlight_style(theme::header_style())
        .divider(Span::styled(
            TAB_DIVIDER,
            Style::new().fg(theme::TEXT_MUTED),
        ));
    frame.render_widget(tabs, tab_area);
    for (i, rect) in tab_rects(tab_inner, View::titles()).into_iter().enumerate() {
        hits.push(rect, Target::Tab(View::from_index(i)));
    }

    if app.inspector.is_some() {
        inspector::render(frame, app, content_area, hits);
    } else {
        render_view(frame, app, content_area, hits);
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

    // Overlays cover the whole screen in the hit map so clicks cannot reach the view
    // behind them.
    let screen = frame.area();
    match &app.input_mode {
        InputMode::CommandPalette => {
            hits.push(screen, Target::Modal(None));
            command_palette::render(frame, app);
        }
        InputMode::Filter => {
            hits.push(screen, Target::Modal(None));
            render_filter_bar(frame, app);
        }
        InputMode::ConfirmKill => {
            hits.push(screen, Target::Modal(Some(Action::ConfirmKill(false))));
            render_kill_confirm(frame, app, hits);
        }
        InputMode::Normal => {}
    }

    if app.show_help {
        hits.push(screen, Target::Modal(Some(Action::ToggleHelp)));
        help::render(frame);
    }
}

/// Click areas of each tab title (with its padding), following how ratatui's `Tabs`
/// lays them out: one space of padding either side, then the divider.
fn tab_rects(inner: Rect, titles: &[&str]) -> Vec<Rect> {
    let mut x = inner.x;
    let right = inner.right();
    let divider = TAB_DIVIDER.chars().count() as u16;
    let mut rects = Vec::with_capacity(titles.len());
    for title in titles {
        if x >= right {
            break;
        }
        let width = (title.chars().count() as u16 + 2).min(right - x);
        rects.push(Rect::new(x, inner.y, width, 1));
        x = x.saturating_add(width + divider);
    }
    rects
}

fn render_view(frame: &mut Frame, app: &mut App, area: Rect, hits: &mut HitMap) {
    match app.view {
        View::Dashboard => dashboard::render(frame, app, area, hits),
        View::CpuDetail => cpu_detail::render(frame, app, area),
        View::MemoryDetail => memory_detail::render(frame, app, area),
        View::DiskDetail => disk_detail::render(frame, app, area),
        View::GpuDetail => gpu_detail::render(frame, app, area),
        View::ProcessTable => process_table::render(frame, app, area, hits),
        View::PortTable => port_table::render(frame, app, area, hits),
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

fn render_kill_confirm(frame: &mut Frame, app: &App, hits: &mut HitMap) {
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

        // Offsets of "[y]es" and "[n]o" in the button line above.
        let buttons_y = popup_area.y + 3;
        let left = popup_area.x + 1;
        hits.push(
            Rect::new(left + 2, buttons_y, 5, 1),
            Target::Button(Action::ConfirmKill(true)),
        );
        hits.push(
            Rect::new(left + 12, buttons_y, 4, 1),
            Target::Button(Action::ConfirmKill(false)),
        );
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

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn tab_rects_cover_rendered_titles() {
        let width = 90;
        let mut terminal = Terminal::new(TestBackend::new(width, 3)).unwrap();
        terminal
            .draw(|frame| {
                let tabs = Tabs::new(View::titles().to_vec())
                    .block(Block::bordered())
                    .divider(Span::raw(TAB_DIVIDER));
                frame.render_widget(tabs, frame.area());
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let inner = Block::bordered().inner(Rect::new(0, 0, width, 3));
        for (rect, title) in tab_rects(inner, View::titles()).iter().zip(View::titles()) {
            let drawn: String = (rect.x + 1..rect.right() - 1)
                .map(|x| buffer[(x, inner.y)].symbol().to_string())
                .collect();
            assert_eq!(&drawn, title);
        }
    }
}
