use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{block::BorderType, Block, Gauge, Paragraph, Sparkline},
    Frame,
};

use crate::app::App;
use crate::theme;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" CPU: {} ", app.system.cpu().cpu_name),
            theme::title_style(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [global_area, sparkline_area, cores_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .areas(inner);

    // Global CPU usage gauge
    let cpu_usage = app.system.cpu().global_usage as f64;
    let ratio = (cpu_usage / 100.0).clamp(0.0, 1.0);

    let gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" Global Usage ", theme::title_style())),
        )
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(cpu_usage))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(ratio)
        .label(format!("{:.1}%", cpu_usage));
    frame.render_widget(gauge, global_area);

    // CPU history sparkline
    let cpu_data: Vec<u64> = app.system.cpu().history.iter().map(|v| *v as u64).collect();
    let sparkline = Sparkline::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" CPU History ", theme::title_style())),
        )
        .data(&cpu_data)
        .max(100)
        .style(Style::new().fg(theme::ORANGE));
    frame.render_widget(sparkline, sparkline_area);

    // Per-core gauges
    let core_count = app.system.cpu().per_core.len().min(32);
    if core_count == 0 {
        let msg = Paragraph::new("No per-core data available").style(theme::dim_style());
        frame.render_widget(msg, cores_area);
        return;
    }

    let core_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" Per-Core ({} cores) ", app.system.cpu().per_core.len()),
            theme::title_style(),
        ));
    let core_inner = core_block.inner(cores_area);
    frame.render_widget(core_block, cores_area);

    let constraints: Vec<Constraint> = (0..core_count)
        .map(|_| Constraint::Length(1))
        .chain(std::iter::once(Constraint::Fill(1)))
        .collect();
    let core_rows = Layout::vertical(constraints).split(core_inner);

    for (i, &usage) in app
        .system
        .cpu()
        .per_core
        .iter()
        .take(core_count)
        .enumerate()
    {
        let u = usage as f64;
        let r = (u / 100.0).clamp(0.0, 1.0);

        let gauge = Gauge::default()
            .gauge_style(Style::new().fg(theme::gauge_color(u)).bg(theme::GAUGE_BG))
            .use_unicode(true)
            .ratio(r)
            .label(format!("Core {:>2}: {:.1}%", i, u));
        frame.render_widget(gauge, core_rows[i]);
    }
}
