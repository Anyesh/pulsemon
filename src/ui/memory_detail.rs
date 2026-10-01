use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{block::BorderType, Block, Gauge, Paragraph, Sparkline},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::types::format_bytes;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" Memory Detail ", theme::title_style()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [ram_area, swap_area, sparkline_area, totals_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .areas(inner);

    // RAM usage gauge
    let mem_total = app.system.memory().total as f64;
    let mem_ratio = if mem_total > 0.0 {
        (app.system.memory().used as f64 / mem_total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mem_pct = mem_ratio * 100.0;

    let ram_gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" RAM ", theme::title_style())),
        )
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(mem_pct))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(mem_ratio)
        .label(format!(
            "{} / {} ({:.1}%)",
            format_bytes(app.system.memory().used),
            format_bytes(app.system.memory().total),
            mem_pct
        ));
    frame.render_widget(ram_gauge, ram_area);

    // Swap usage gauge
    let swap_total = app.system.memory().swap_total as f64;
    let swap_ratio = if swap_total > 0.0 {
        (app.system.memory().swap_used as f64 / swap_total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let swap_pct = swap_ratio * 100.0;

    let swap_gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" Swap ", theme::title_style())),
        )
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(swap_pct))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(swap_ratio)
        .label(format!(
            "{} / {} ({:.1}%)",
            format_bytes(app.system.memory().swap_used),
            format_bytes(app.system.memory().swap_total),
            swap_pct
        ));
    frame.render_widget(swap_gauge, swap_area);

    // Memory history sparkline
    let mem_data: Vec<u64> = app
        .system
        .memory()
        .history
        .iter()
        .map(|v| *v as u64)
        .collect();
    let sparkline = Sparkline::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" Memory History ", theme::title_style())),
        )
        .data(&mem_data)
        .max(100)
        .style(Style::new().fg(theme::ORANGE));
    frame.render_widget(sparkline, sparkline_area);

    // System totals — clean text with dim labels and highlighted values
    let totals_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" System Totals ", theme::title_style()));

    let free_ram = app
        .system
        .memory()
        .total
        .saturating_sub(app.system.memory().used);
    let free_swap = app
        .system
        .memory()
        .swap_total
        .saturating_sub(app.system.memory().swap_used);

    let totals_text = vec![
        Line::from(vec![
            Span::styled("Total RAM:  ", theme::dim_style()),
            Span::styled(format_bytes(app.system.memory().total), theme::text_style()),
        ]),
        Line::from(vec![
            Span::styled("Used RAM:   ", theme::dim_style()),
            Span::styled(format_bytes(app.system.memory().used), theme::text_style()),
        ]),
        Line::from(vec![
            Span::styled("Free RAM:   ", theme::dim_style()),
            Span::styled(format_bytes(free_ram), theme::text_style()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Total Swap: ", theme::dim_style()),
            Span::styled(
                format_bytes(app.system.memory().swap_total),
                theme::text_style(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Used Swap:  ", theme::dim_style()),
            Span::styled(
                format_bytes(app.system.memory().swap_used),
                theme::text_style(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Free Swap:  ", theme::dim_style()),
            Span::styled(format_bytes(free_swap), theme::text_style()),
        ]),
    ];

    let totals = Paragraph::new(totals_text).block(totals_block);
    frame.render_widget(totals, totals_area);
}
