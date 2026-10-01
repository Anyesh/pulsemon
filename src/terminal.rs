use std::io::{self, stdout};
use std::panic;

use crossterm::execute;
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::{DefaultTerminal, Terminal};

/// Owns terminal setup and teardown. `ratatui::init` is not used because its panic
/// hook restores the terminal before ours could run, and any terminal mode we add
/// must be undone before raw mode is turned off.
pub struct TerminalGuard {
    terminal: DefaultTerminal,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        Ok(Self { terminal })
    }

    pub fn terminal(&mut self) -> &mut DefaultTerminal {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        teardown();
    }
}

fn teardown() {
    ratatui::restore();
}

fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        teardown();
        default_hook(info);
        // A panic on the collector or input thread would otherwise leave the UI
        // running on a terminal that has already been restored.
        std::process::exit(101);
    }));
}
