use std::fmt;
use std::io::{self, stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::execute;
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::Command;
use ratatui::backend::CrosstermBackend;
use ratatui::{DefaultTerminal, Terminal};

/// Read by the panic hook, which cannot reach the guard.
static MOUSE_ON: AtomicBool = AtomicBool::new(false);

/// Owns terminal setup and teardown. `ratatui::init` is not used because its panic
/// hook restores the terminal before ours could run, and mouse capture must be
/// turned off before raw mode: on Windows, disabling capture writes back the console
/// mode saved when capture was enabled, which was already raw.
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

    pub fn mouse(&self) -> bool {
        MOUSE_ON.load(Ordering::SeqCst)
    }

    pub fn set_mouse(&mut self, on: bool) -> io::Result<()> {
        if on == self.mouse() {
            return Ok(());
        }
        if on {
            execute!(stdout(), EnableClicks)?;
        } else {
            execute!(stdout(), DisableClicks)?;
        }
        MOUSE_ON.store(on, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        teardown();
    }
}

fn teardown() {
    if MOUSE_ON.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout(), DisableClicks);
    }
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

/// Mouse reporting for presses, releases and the wheel only (modes 1000 and 1006).
/// crossterm's `EnableMouseCapture` also turns on motion tracking, which floods the
/// input queue with an event for every cell the pointer crosses.
struct EnableClicks;

impl Command for EnableClicks {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1000h\x1b[?1006h")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        crossterm::event::EnableMouseCapture.execute_winapi()
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

struct DisableClicks;

impl Command for DisableClicks {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1006l\x1b[?1000l")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        crossterm::event::DisableMouseCapture.execute_winapi()
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}
