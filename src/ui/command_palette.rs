use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let height = if app.command_error.is_some() { 4 } else { 3 };
    let palette_area = Rect {
        x: 0,
        y: area.height.saturating_sub(height),
        width: area.width,
        height: height.min(area.height),
    };

    frame.render_widget(Clear, palette_area);

    let title = Line::from(vec![
        Span::styled(" Command ", theme::title_style()),
        Span::styled("(Esc to cancel) ", theme::dim_style()),
    ]);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::active_border_style())
        .title(title);

    let inner = block.inner(palette_area);
    frame.render_widget(block, palette_area);

    if let Some(ref err) = app.command_error {
        let [input_area, error_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

        let input = Paragraph::new(Line::from(vec![
            Span::styled(":", theme::dim_style()),
            Span::styled(app.command_input.clone(), Style::new().fg(theme::TEXT)),
        ]));
        frame.render_widget(input, input_area);

        let error = Paragraph::new(Span::styled(err.clone(), Style::new().fg(theme::RED)));
        frame.render_widget(error, error_area);
    } else {
        let input = Paragraph::new(Line::from(vec![
            Span::styled(":", theme::dim_style()),
            Span::styled(app.command_input.clone(), Style::new().fg(theme::TEXT)),
        ]));
        frame.render_widget(input, inner);
    }
}
