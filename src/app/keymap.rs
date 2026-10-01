use crossterm::event::{KeyCode, KeyEvent};

use super::action::Action;
use super::View;

pub const PAGE: i32 = 20;

pub fn normal(key: KeyEvent) -> Option<Action> {
    Some(match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::Back,
        KeyCode::Char('?') => Action::ToggleHelp,
        KeyCode::Char(':') => Action::OpenPalette,
        KeyCode::Char('/') => Action::OpenFilter,
        KeyCode::Char(c @ '1'..='7') => {
            Action::SwitchView(View::from_index(c as usize - '1' as usize))
        }
        KeyCode::Tab => Action::CycleView(1),
        KeyCode::BackTab => Action::CycleView(-1),
        KeyCode::Down | KeyCode::Char('j') => Action::MoveSelection(1),
        KeyCode::Up | KeyCode::Char('k') => Action::MoveSelection(-1),
        KeyCode::PageDown => Action::MoveSelection(PAGE),
        KeyCode::PageUp => Action::MoveSelection(-PAGE),
        KeyCode::Home => Action::SelectFirst,
        KeyCode::End => Action::SelectLast,
        KeyCode::Char('s') => Action::CycleSort,
        KeyCode::Char('S') => Action::ToggleSortDir,
        KeyCode::Delete | KeyCode::Char('K') => Action::RequestKill,
        KeyCode::Char('+') | KeyCode::Char('=') => Action::AdjustRate(-250),
        KeyCode::Char('-') => Action::AdjustRate(250),
        KeyCode::Enter => Action::OpenInspector,
        _ => return None,
    })
}

/// Keys while the inspector is open; anything not listed falls back to `normal`.
pub fn inspector(key: KeyEvent) -> Option<Action> {
    Some(match key.code {
        KeyCode::Esc => Action::CloseInspector,
        KeyCode::Backspace => Action::InspectorBack,
        KeyCode::Enter => Action::InspectLink,
        KeyCode::Char('e') => Action::ToggleEnv,
        KeyCode::PageDown => Action::ScrollInspector(PAGE / 2),
        KeyCode::PageUp => Action::ScrollInspector(-PAGE / 2),
        _ => return normal(key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn digits_jump_to_views() {
        assert_eq!(
            normal(key(KeyCode::Char('1'))),
            Some(Action::SwitchView(View::Dashboard))
        );
        assert_eq!(
            normal(key(KeyCode::Char('7'))),
            Some(Action::SwitchView(View::PortTable))
        );
        assert_eq!(normal(key(KeyCode::Char('8'))), None);
    }

    #[test]
    fn inspector_keys_fall_back_to_normal() {
        assert_eq!(inspector(key(KeyCode::Esc)), Some(Action::CloseInspector));
        assert_eq!(
            inspector(key(KeyCode::Char('j'))),
            Some(Action::MoveSelection(1))
        );
        assert_eq!(inspector(key(KeyCode::Char('q'))), Some(Action::Quit));
    }

    #[test]
    fn plus_speeds_up_refresh() {
        assert_eq!(
            normal(key(KeyCode::Char('+'))),
            Some(Action::AdjustRate(-250))
        );
    }
}
