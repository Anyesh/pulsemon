use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{block::BorderType, Block, Cell, Gauge, Paragraph, Row, Sparkline, Table},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::types::format_bytes;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let [top_area, sparkline_area, disk_gpu_area, process_area] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .areas(area);

    render_top_gauges(frame, app, top_area);
    render_sparklines(frame, app, sparkline_area);
    render_disk_gpu(frame, app, disk_gpu_area);
    render_mini_process_table(frame, app, process_area);
}

fn render_top_gauges(frame: &mut Frame, app: &App, area: Rect) {
    let [cpu_area, mem_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    // CPU gauge
    let cpu_usage = app.data.cpu.global_usage as f64;
    let cpu_ratio = (cpu_usage / 100.0).clamp(0.0, 1.0);
    let cpu_gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Line::from(vec![Span::styled(
                    " CPU ",
                    theme::title_style(),
                )])),
        )
        .gauge_style(
            Style::new()
                .fg(theme::gauge_color(cpu_usage))
                .bg(theme::GAUGE_BG),
        )
        .use_unicode(true)
        .ratio(cpu_ratio)
        .label(format!("{:.1}%", cpu_usage));
    frame.render_widget(cpu_gauge, cpu_area);

    // Memory gauge
    let mem_total = app.data.memory.total as f64;
    let mem_ratio = if mem_total > 0.0 {
        (app.data.memory.used as f64 / mem_total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mem_pct = mem_ratio * 100.0;
    let mem_gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Line::from(vec![Span::styled(
                    " Memory ",
                    theme::title_style(),
                )])),
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
            format_bytes(app.data.memory.used),
            format_bytes(app.data.memory.total),
            mem_pct
        ));
    frame.render_widget(mem_gauge, mem_area);
}

fn render_sparklines(frame: &mut Frame, app: &App, area: Rect) {
    let [cpu_area, mem_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let cpu_data: Vec<u64> = app.data.cpu.history.iter().map(|v| *v as u64).collect();
    let cpu_sparkline = Sparkline::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" CPU ", theme::title_style())),
        )
        .data(&cpu_data)
        .max(100)
        .style(Style::new().fg(theme::ORANGE));
    frame.render_widget(cpu_sparkline, cpu_area);

    let mem_data: Vec<u64> = app.data.memory.history.iter().map(|v| *v as u64).collect();
    let mem_sparkline = Sparkline::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::border_style())
                .title(Span::styled(" Memory ", theme::title_style())),
        )
        .data(&mem_data)
        .max(100)
        .style(Style::new().fg(theme::ORANGE));
    frame.render_widget(mem_sparkline, mem_area);
}

fn render_disk_gpu(frame: &mut Frame, app: &App, area: Rect) {
    let [disk_area, gpu_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    // Disk section
    let disk_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" Disks ", theme::title_style()));
    let disk_inner = disk_block.inner(disk_area);
    frame.render_widget(disk_block, disk_area);

    let disk_count = app.data.disks.len().min(3);
    if disk_count > 0 {
        let constraints: Vec<Constraint> = (0..disk_count)
            .map(|_| Constraint::Length(1))
            .chain(std::iter::once(Constraint::Fill(1)))
            .collect();
        let disk_rows = Layout::vertical(constraints).split(disk_inner);

        for (i, disk) in app.data.disks.iter().take(3).enumerate() {
            let total = disk.total as f64;
            let ratio = if total > 0.0 {
                (disk.used as f64 / total).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let pct = ratio * 100.0;
            let label = format!(
                "{}: {} / {}",
                disk.name,
                format_bytes(disk.used),
                format_bytes(disk.total)
            );
            let gauge = Gauge::default()
                .gauge_style(Style::new().fg(theme::gauge_color(pct)).bg(theme::GAUGE_BG))
                .use_unicode(true)
                .ratio(ratio)
                .label(label);
            frame.render_widget(gauge, disk_rows[i]);
        }
    } else {
        let msg = Paragraph::new("No disks detected").style(theme::dim_style());
        frame.render_widget(msg, disk_inner);
    }

    // GPU section
    let gpu_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(Span::styled(" GPU ", theme::title_style()));
    let gpu_inner = gpu_block.inner(gpu_area);
    frame.render_widget(gpu_block, gpu_area);

    if app.data.gpu.is_empty() {
        let msg = Paragraph::new("No GPU detected").style(theme::dim_style());
        frame.render_widget(msg, gpu_inner);
    } else {
        let gpu = &app.data.gpu[0];
        let utilization = gpu.utilization.unwrap_or(0.0) as f64;
        let ratio = (utilization / 100.0).clamp(0.0, 1.0);

        let [name_area, gauge_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(gpu_inner);

        let name_text = Paragraph::new(gpu.name.clone())
            .style(Style::new().fg(theme::ORANGE).add_modifier(Modifier::BOLD));
        frame.render_widget(name_text, name_area);

        let gpu_gauge = Gauge::default()
            .gauge_style(
                Style::new()
                    .fg(theme::gauge_color(utilization))
                    .bg(theme::GAUGE_BG),
            )
            .use_unicode(true)
            .ratio(ratio)
            .label(format!("Util: {:.1}%", utilization));
        frame.render_widget(gpu_gauge, gauge_area);
    }
}

fn render_mini_process_table(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("PID"),
        Cell::from("Name"),
        Cell::from("CPU%"),
        Cell::from("Memory"),
    ])
    .style(theme::header_style());

    let procs = &app.data.processes;
    let rows: Vec<Row> = app
        .process_view
        .top
        .iter()
        .map(|&i| &procs[i])
        .enumerate()
        .map(|(i, p)| {
            let style = if i % 2 == 1 {
                Style::new().bg(theme::BG_ALT_ROW).fg(theme::TEXT)
            } else {
                theme::text_style()
            };
            Row::new(vec![
                Cell::from(p.pid().to_string()),
                Cell::from(&*p.name),
                Cell::from(format!("{:.1}", p.cpu_usage)),
                Cell::from(format_bytes(p.memory)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Fill(1),
        Constraint::Length(8),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::border_style())
            .title(Span::styled(" Top Processes ", theme::title_style())),
    );

    frame.render_widget(table, area);
}
