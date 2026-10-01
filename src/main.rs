mod app;
mod collectors;
mod config;
mod event;
mod schedule;
mod terminal;
mod theme;
mod types;
mod ui;
mod views;

use std::time::Instant;

use anyhow::Result;
use clap::Parser;

use app::App;
use config::Config;
use event::{AppEvent, EventHandler};
use terminal::TerminalGuard;

fn main() -> Result<()> {
    let config = Config::parse();
    let mut guard = TerminalGuard::new()?;
    run(guard.terminal(), &config)
}

fn run(terminal: &mut ratatui::DefaultTerminal, config: &Config) -> Result<()> {
    let mut app = App::new(config);
    let events = EventHandler::new();
    let mut last_tick = Instant::now();
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|frame| ui::render(frame, &mut app))?;
            dirty = false;
        }

        let wait = schedule::wait_for(
            Instant::now(),
            last_tick,
            app.tick_rate,
            app.status_deadline(),
        );
        // Drain everything queued before drawing so a burst of input costs one redraw.
        let mut next = events.recv_timeout(wait)?;
        while let Some(event) = next {
            match event {
                AppEvent::Key(key) => app.handle_key(key),
                AppEvent::Resize => {}
            }
            dirty = true;
            next = events.try_recv()?;
        }

        if !app.running {
            return Ok(());
        }

        let now = Instant::now();
        if schedule::tick_due(now, last_tick, app.tick_rate) {
            app.refresh_all();
            last_tick = now;
            dirty = true;
        }
        dirty |= app.expire_status(now);
    }
}
