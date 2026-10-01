mod app;
mod collectors;
mod config;
mod event;
mod theme;
mod types;
mod ui;

use std::panic;

use anyhow::Result;
use clap::Parser;

use app::App;
use config::Config;
use event::{AppEvent, EventHandler};

fn main() -> Result<()> {
    // Set panic hook to restore terminal before printing panic info
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let config = Config::parse();

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &config);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, config: &Config) -> Result<()> {
    let mut app = App::new(config);
    let events = EventHandler::new(app.tick_rate);

    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        match events.next()? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => app.refresh_all(),
            AppEvent::Resize(_, _) => {} // ratatui handles redraw
        }

        if !app.running {
            return Ok(());
        }
    }
}
