use ratatui::style::{Color, Modifier, Style};

// Claude Code inspired orange palette
pub const ORANGE: Color = Color::Rgb(232, 131, 58);
pub const ORANGE_DIM: Color = Color::Rgb(180, 100, 40);
pub const ORANGE_BRIGHT: Color = Color::Rgb(255, 160, 80);

// Neutrals
pub const TEXT: Color = Color::Rgb(210, 210, 210);
pub const TEXT_DIM: Color = Color::Rgb(120, 120, 120);
pub const TEXT_MUTED: Color = Color::Rgb(80, 80, 80);
pub const BORDER: Color = Color::Rgb(60, 60, 60);
pub const BORDER_ACTIVE: Color = ORANGE_DIM;
pub const BG_HIGHLIGHT: Color = Color::Rgb(35, 35, 35);
pub const BG_ALT_ROW: Color = Color::Rgb(28, 28, 28);

// Semantic colors — kept soft
pub const RED: Color = Color::Rgb(220, 100, 100);

// Gauge colors — orange gradient
pub const GAUGE_LOW: Color = Color::Rgb(100, 180, 140); // soft teal-green
pub const GAUGE_MID: Color = ORANGE; // orange
pub const GAUGE_HIGH: Color = Color::Rgb(220, 80, 80); // soft red
pub const GAUGE_BG: Color = Color::Rgb(40, 40, 40);

pub fn gauge_color(pct: f64) -> Color {
    if pct < 50.0 {
        GAUGE_LOW
    } else if pct < 80.0 {
        GAUGE_MID
    } else {
        GAUGE_HIGH
    }
}

// Pre-built styles
pub fn title_style() -> Style {
    Style::new().fg(ORANGE).add_modifier(Modifier::BOLD)
}

pub fn border_style() -> Style {
    Style::new().fg(BORDER)
}

pub fn active_border_style() -> Style {
    Style::new().fg(BORDER_ACTIVE)
}

pub fn header_style() -> Style {
    Style::new().fg(ORANGE).add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::new()
        .bg(ORANGE_DIM)
        .fg(Color::Rgb(240, 240, 240))
        .add_modifier(Modifier::BOLD)
}

pub fn dim_style() -> Style {
    Style::new().fg(TEXT_DIM)
}

pub fn text_style() -> Style {
    Style::new().fg(TEXT)
}
