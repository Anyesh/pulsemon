use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
    Frame,
};

use crate::theme;

const SECTIONS: [(&str, &[(&str, &str)]); 5] = [
    (
        "Navigation",
        &[
            ("1-7", "Jump to view"),
            ("Tab / S-Tab", "Cycle views"),
            ("Esc", "Clear filter, back to dashboard, quit"),
        ],
    ),
    (
        "Tables",
        &[
            ("j/k  \u{2191}/\u{2193}", "Move selection"),
            ("PgUp / PgDn", "Move a page"),
            ("s / S", "Next sort column / flip direction"),
            ("/", "Filter"),
            ("Enter", "Inspect selected process"),
            ("K / Del", "Send a signal"),
        ],
    ),
    (
        "Inspector",
        &[
            ("j/k  Enter", "Pick and open a parent or child"),
            ("Backspace", "Back to the previous process"),
            ("e", "Show or hide environment"),
            ("PgUp / PgDn", "Scroll"),
            ("Esc", "Close"),
        ],
    ),
    (
        "Mouse",
        &[
            ("click", "Tabs, rows, headers (again to flip)"),
            ("double-click", "Inspect a row"),
            ("right-click", "Signal menu for a row"),
            ("wheel", "Scroll the table under the pointer"),
            ("m", "Mouse on/off; Shift+drag selects text"),
        ],
    ),
    (
        "Actions",
        &[
            (":", "Command palette"),
            ("+ / -", "Faster / slower refresh"),
            ("?", "Toggle this help"),
            ("q", "Quit"),
        ],
    ),
];

pub fn render(frame: &mut Frame) {
    let key_style = Style::new().fg(theme::TEXT);
    let desc_style = theme::dim_style();

    let mut lines = Vec::new();
    for (title, keys) in SECTIONS {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            theme::header_style(),
        )));
        for (key, desc) in keys {
            lines.push(Line::from(vec![
                Span::styled(format!("    {key:<16}"), key_style),
                Span::styled(*desc, desc_style),
            ]));
        }
    }

    let area = frame.area();
    let width = 64u16.min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let title = Line::from(vec![
        Span::styled(" Help ", theme::title_style()),
        Span::styled("keys and mouse ", theme::dim_style()),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border_style())
        .title(title);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
