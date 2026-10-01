use std::fmt::Display;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Sparkline},
    Frame,
};

use crate::app::App;
use crate::collectors::inspect::{Account, Field, Missing, ProcessDetail};
use crate::theme;
use crate::types::{format_bytes, format_elapsed, format_utc};
use crate::views::inspector::{ChainEnd, InspectorView, Link};

const LABEL_WIDTH: usize = 13;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.inspector.as_mut() else {
        return;
    };

    let name = view.row.as_ref().map_or("?", |r| &*r.name);
    let title = Line::from(vec![
        Span::styled(" Inspect ", theme::title_style()),
        Span::styled(format!("{} ({}) ", name, view.key.pid), theme::text_style()),
    ]);
    let hints = Line::from(Span::styled(
        " Esc close \u{00b7} Bksp back \u{00b7} \u{2191}\u{2193} Enter follow \u{00b7} e env \u{00b7} K signal ",
        theme::dim_style(),
    ));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::active_border_style())
        .title(title)
        .title_bottom(hints);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stats_area, spark_area, body_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(inner);

    frame.render_widget(Paragraph::new(stats_line(view)), stats_area);
    render_sparklines(frame, view, spark_area);

    let (lines, link_lines) = body_lines(view);
    let height = body_area.height;
    let max_scroll = (lines.len() as u16).saturating_sub(height);
    if std::mem::take(&mut view.reveal_link) {
        if let Some(&line) = link_lines.get(view.link_cursor) {
            if line < view.scroll {
                view.scroll = line;
            } else if line >= view.scroll + height {
                view.scroll = line + 1 - height;
            }
        }
    }
    view.scroll = view.scroll.min(max_scroll);
    frame.render_widget(Paragraph::new(lines).scroll((view.scroll, 0)), body_area);
}

fn stats_line(view: &InspectorView) -> Line<'static> {
    let Some(row) = &view.row else {
        return Line::from(Span::styled(
            " process has exited",
            Style::new().fg(theme::RED),
        ));
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let started = view.key.start_time;
    Line::from(vec![
        Span::styled(" Status ", theme::dim_style()),
        Span::styled(row.status, theme::text_style()),
        Span::styled("   CPU ", theme::dim_style()),
        Span::styled(format!("{:.1}%", row.cpu_usage), theme::text_style()),
        Span::styled("   Memory ", theme::dim_style()),
        Span::styled(format_bytes(row.memory), theme::text_style()),
        Span::styled("   Disk ", theme::dim_style()),
        Span::styled(
            format!("{}/s", format_bytes(row.disk_rate)),
            theme::text_style(),
        ),
        Span::styled("   Up ", theme::dim_style()),
        Span::styled(
            format_elapsed(now.saturating_sub(started)),
            theme::text_style(),
        ),
        Span::styled(
            format!("   since {}", format_utc(started)),
            theme::dim_style(),
        ),
    ])
}

fn render_sparklines(frame: &mut Frame, view: &InspectorView, area: Rect) {
    let [cpu_area, mem_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
    let cpu: Vec<u64> = view.cpu.iter().copied().collect();
    let memory: Vec<u64> = view.memory.iter().copied().collect();
    let spark = |title: &'static str, data: &[u64]| {
        Sparkline::default()
            .block(Block::new().title(Span::styled(title, theme::dim_style())))
            .data(data.to_vec())
            .style(Style::new().fg(theme::ORANGE))
    };
    frame.render_widget(spark(" CPU since opened", &cpu).max(100), cpu_area);
    frame.render_widget(spark(" Memory since opened", &memory), mem_area);
}

/// Builds the scrollable body. Also returns the line index of every link (ancestors
/// then children) so the focused one can be scrolled into view.
fn body_lines(view: &InspectorView) -> (Vec<Line<'static>>, Vec<u16>) {
    let mut out = Body::default();
    let detail = view.detail.as_ref();

    out.section("Identity");
    match detail {
        Some(d) => identity(&mut out, d),
        None => out.note("collecting\u{2026}"),
    }

    out.section("Ownership");
    if let Some(d) = detail {
        ownership(&mut out, d, view);
    }

    out.section("Lineage");
    lineage(&mut out, view);

    out.section(&format!("Children ({})", view.children.len()));
    let ancestors = view.lineage.ancestors.len();
    if view.children.is_empty() {
        out.note("none");
    }
    for (i, child) in view.children.iter().enumerate() {
        out.link(child, view.link_cursor == ancestors + i, None);
    }

    if let Some(d) = detail {
        ports(&mut out, d);
        environment(&mut out, d, view.show_env);
    }
    (out.lines, out.links)
}

fn identity(out: &mut Body, d: &ProcessDetail) {
    out.field("Executable", field(&d.exe));
    let argv: Field<String> = d.argv.as_ref().map(|a| a.join(" ")).map_err(|m| *m);
    out.field("Arguments", field(&argv));
    out.field("Working dir", field(&d.cwd));
    if let Ok(root) = &d.root {
        if root != "/" {
            out.field("Root", vec![Span::styled(root.clone(), warn_style())]);
        }
    }
}

fn ownership(out: &mut Body, d: &ProcessDetail, view: &InspectorView) {
    out.field("User", account(&d.user));
    if let (Ok(real), Ok(effective)) = (&d.user, &d.effective_user) {
        if real.id != effective.id {
            let mut spans = account(&d.effective_user);
            spans.push(Span::styled("  setuid", warn_style()));
            out.field("Effective", spans);
        }
    }
    out.field("Group", account(&d.group));
    out.field("Session", field(&d.session));
    if let Some(row) = &view.row {
        if let Some(parent) = row.parent {
            out.field(
                "Parent pid",
                vec![Span::styled(parent.to_string(), theme::text_style())],
            );
        }
    }
}

fn lineage(out: &mut Body, view: &InspectorView) {
    let l = &view.lineage;
    if l.ancestors.is_empty() && l.end == ChainEnd::Root {
        out.note("no parent");
    }
    for (i, link) in l.ancestors.iter().enumerate() {
        let note = (i == 0 && l.adopted)
            .then_some("adopted: started by the init system, or its original launcher exited");
        out.link(link, view.link_cursor == i, note);
    }
    match l.end {
        ChainEnd::Root => {}
        ChainEnd::ParentExited(pid) => out.warn(&format!(
            "parent {pid} has exited, so the launcher is unknown"
        )),
        ChainEnd::PidReused(pid) => out.warn(&format!(
            "parent {pid} exited and its pid now belongs to a newer process"
        )),
    }
}

fn ports(out: &mut Body, d: &ProcessDetail) {
    match &d.ports {
        Ok(ports) => {
            out.section(&format!("Ports ({})", ports.len()));
            if ports.is_empty() {
                out.note("none");
            }
            for p in ports {
                let remote = if p.remote_port == 0 {
                    String::new()
                } else {
                    format!(" \u{2192} {}:{}", p.remote_addr, p.remote_port)
                };
                out.line(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<4}", p.protocol), theme::dim_style()),
                    Span::styled(
                        format!("{}:{}", p.local_addr, p.local_port),
                        theme::text_style(),
                    ),
                    Span::styled(remote, theme::dim_style()),
                    Span::styled(format!("  {}", p.state), theme::dim_style()),
                ]);
            }
        }
        Err(missing) => {
            out.section("Ports");
            out.line(vec![Span::raw("  "), missing_span(*missing)]);
        }
    }
}

fn environment(out: &mut Body, d: &ProcessDetail, show: bool) {
    match &d.environ {
        Ok(vars) if show => {
            out.section(&format!("Environment ({})", vars.len()));
            for var in vars {
                out.line(vec![
                    Span::raw("  "),
                    Span::styled(var.clone(), theme::dim_style()),
                ]);
            }
        }
        Ok(vars) => {
            out.section(&format!("Environment ({})", vars.len()));
            out.note("hidden because it often holds secrets; press e to show");
        }
        Err(missing) => {
            out.section("Environment");
            out.line(vec![Span::raw("  "), missing_span(*missing)]);
        }
    }
}

#[derive(Default)]
struct Body {
    lines: Vec<Line<'static>>,
    links: Vec<u16>,
}

impl Body {
    fn line(&mut self, spans: Vec<Span<'static>>) {
        self.lines.push(Line::from(spans));
    }

    fn section(&mut self, title: &str) {
        if !self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.line(vec![Span::styled(title.to_string(), theme::header_style())]);
    }

    fn field(&mut self, label: &str, value: Vec<Span<'static>>) {
        let mut spans = vec![Span::styled(
            format!("  {label:<LABEL_WIDTH$}"),
            theme::dim_style(),
        )];
        spans.extend(value);
        self.line(spans);
    }

    fn note(&mut self, text: &str) {
        self.line(vec![Span::styled(format!("  {text}"), theme::dim_style())]);
    }

    fn warn(&mut self, text: &str) {
        self.line(vec![Span::styled(format!("  {text}"), warn_style())]);
    }

    fn link(&mut self, link: &Link, focused: bool, note: Option<&str>) {
        self.links.push(self.lines.len() as u16);
        let style = if focused {
            theme::selected_style()
        } else {
            theme::text_style()
        };
        let marker = if focused { "\u{25b8} " } else { "  " };
        let mut spans = vec![
            Span::styled(marker, Style::new().fg(theme::ORANGE)),
            Span::styled(format!("{} ({})", link.name, link.key.pid), style),
        ];
        if let Some(note) = note {
            spans.push(Span::styled(format!("  {note}"), warn_style()));
        }
        self.line(spans);
    }
}

fn field<T: Display>(value: &Field<T>) -> Vec<Span<'static>> {
    match value {
        Ok(v) => vec![Span::styled(v.to_string(), theme::text_style())],
        Err(missing) => vec![missing_span(*missing)],
    }
}

fn account(value: &Field<Account>) -> Vec<Span<'static>> {
    match value {
        Ok(a) => vec![
            Span::styled(a.name.clone(), theme::text_style()),
            Span::styled(format!(" ({})", a.id), theme::dim_style()),
        ],
        Err(missing) => vec![missing_span(*missing)],
    }
}

fn missing_span(missing: Missing) -> Span<'static> {
    match missing {
        Missing::Denied => {
            Span::styled("denied", theme::dim_style().add_modifier(Modifier::ITALIC))
        }
        Missing::Unavailable => Span::styled("-", theme::dim_style()),
    }
}

fn warn_style() -> Style {
    Style::new().fg(theme::ORANGE_BRIGHT)
}
