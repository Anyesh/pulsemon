use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
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
        .title(Span::styled(" Disk Detail ", theme::title_style()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.data.disks.is_empty() {
        let msg = Paragraph::new("No disks detected").style(theme::dim_style());
        frame.render_widget(msg, inner);
        return;
    }

    // Each disk gets 2 lines: label + gauge, plus 1 spacing line between disks
    let constraints: Vec<Constraint> = app
        .data
        .disks
        .iter()
        .enumerate()
        .flat_map(|(i, _)| {
            let mut v = vec![Constraint::Length(1), Constraint::Length(1)];
            if i < app.data.disks.len() - 1 {
                v.push(Constraint::Length(1)); // spacing
            }
            v
        })
        .chain(std::iter::once(Constraint::Fill(1)))
        .collect();
    let rows = Layout::vertical(constraints).split(inner);

    let mut row_idx = 0;
    for (i, disk) in app.data.disks.iter().enumerate() {
        if row_idx + 1 >= rows.len() {
            break;
        }

        let total = disk.total as f64;
        let ratio = if total > 0.0 {
            (disk.used as f64 / total).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pct = ratio * 100.0;

        // Label line: mount point and filesystem type in dim text
        let label_line = Line::from(vec![
            Span::styled(&disk.name, theme::text_style()),
            Span::styled("  ", theme::dim_style()),
            Span::styled(&disk.mount_point, theme::dim_style()),
            Span::styled(" [", theme::dim_style()),
            Span::styled(&disk.fs_type, theme::dim_style()),
            Span::styled("]", theme::dim_style()),
        ]);
        let label = Paragraph::new(label_line);
        frame.render_widget(label, rows[row_idx]);

        // Gauge line
        let gauge = Gauge::default()
            .gauge_style(Style::new().fg(theme::gauge_color(pct)).bg(theme::GAUGE_BG))
            .use_unicode(true)
            .ratio(ratio)
            .label(format!(
                "{} / {} ({:.1}%)",
                format_bytes(disk.used),
                format_bytes(disk.total),
                pct
            ));
        frame.render_widget(gauge, rows[row_idx + 1]);

        // Advance past label + gauge + spacing
        row_idx += if i < app.data.disks.len() - 1 { 3 } else { 2 };
    }
}
