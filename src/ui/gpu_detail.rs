use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{block::BorderType, Block, Gauge, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::types::format_bytes;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" GPU Detail ", theme::title_style()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.gpu_metrics.is_empty() {
        let msg = Paragraph::new("No supported GPU detected").style(theme::dim_style());
        frame.render_widget(msg, inner);
        return;
    }

    // Each GPU section: name(1) + util gauge(1) + vram gauge(1) + info(3) + spacing(1) = 7 lines
    let constraints: Vec<Constraint> = app
        .gpu_metrics
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i < app.gpu_metrics.len() - 1 {
                Constraint::Length(8)
            } else {
                Constraint::Length(7)
            }
        })
        .chain(std::iter::once(Constraint::Fill(1)))
        .collect();
    let gpu_sections = Layout::vertical(constraints).split(inner);

    for (i, gpu) in app.gpu_metrics.iter().enumerate() {
        if i >= gpu_sections.len() - 1 {
            break;
        }
        render_single_gpu(frame, gpu, gpu_sections[i]);
    }
}

fn render_single_gpu(frame: &mut Frame, gpu: &crate::types::GpuMetrics, area: Rect) {
    let [name_area, util_area, vram_area, info_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    // GPU name in orange bold
    let name = Paragraph::new(gpu.name.clone())
        .style(Style::new().fg(theme::ORANGE).add_modifier(Modifier::BOLD));
    frame.render_widget(name, name_area);

    // Utilization gauge
    let util = gpu.utilization.unwrap_or(0.0) as f64;
    let util_ratio = (util / 100.0).clamp(0.0, 1.0);
    let util_label = match gpu.utilization {
        Some(u) => format!("Utilization: {:.1}%", u),
        None => "Utilization: N/A".to_string(),
    };
    let util_gauge = Gauge::default()
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(util))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(util_ratio)
        .label(util_label);
    frame.render_widget(util_gauge, util_area);

    // VRAM gauge
    let (vram_ratio, vram_pct, vram_label) = match (gpu.memory_used, gpu.memory_total) {
        (Some(used), Some(total)) if total > 0 => {
            let ratio = (used as f64 / total as f64).clamp(0.0, 1.0);
            let label = format!(
                "VRAM: {} / {} ({:.1}%)",
                format_bytes(used),
                format_bytes(total),
                ratio * 100.0
            );
            (ratio, ratio * 100.0, label)
        }
        _ => (0.0, 0.0, "VRAM: N/A".to_string()),
    };
    let vram_gauge = Gauge::default()
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(vram_pct))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(vram_ratio)
        .label(vram_label);
    frame.render_widget(vram_gauge, vram_area);

    // Temperature, Power, Fan — clean dim labels with highlighted values
    let temp_line = Line::from(match gpu.temperature {
        Some(t) => vec![
            Span::styled("Temperature: ", theme::dim_style()),
            Span::styled(format!("{:.0}C", t), theme::text_style()),
        ],
        None => vec![
            Span::styled("Temperature: ", theme::dim_style()),
            Span::styled("N/A", theme::dim_style()),
        ],
    });

    let power_line = Line::from(match (gpu.power_usage, gpu.power_limit) {
        (Some(usage), Some(limit)) => vec![
            Span::styled("Power: ", theme::dim_style()),
            Span::styled(format!("{:.1}W", usage), theme::text_style()),
            Span::styled(" / ", theme::dim_style()),
            Span::styled(format!("{:.1}W", limit), theme::text_style()),
        ],
        (Some(usage), None) => vec![
            Span::styled("Power: ", theme::dim_style()),
            Span::styled(format!("{:.1}W", usage), theme::text_style()),
        ],
        (None, Some(limit)) => vec![
            Span::styled("Power: ", theme::dim_style()),
            Span::styled("N/A", theme::dim_style()),
            Span::styled(" / ", theme::dim_style()),
            Span::styled(format!("{:.1}W", limit), theme::text_style()),
        ],
        (None, None) => vec![
            Span::styled("Power: ", theme::dim_style()),
            Span::styled("N/A", theme::dim_style()),
        ],
    });

    let fan_line = Line::from(match gpu.fan_speed {
        Some(speed) => vec![
            Span::styled("Fan Speed: ", theme::dim_style()),
            Span::styled(format!("{}%", speed), theme::text_style()),
        ],
        None => vec![
            Span::styled("Fan Speed: ", theme::dim_style()),
            Span::styled("N/A", theme::dim_style()),
        ],
    });

    let info = Paragraph::new(vec![temp_line, power_line, fan_line]);
    frame.render_widget(info, info_area);
}
