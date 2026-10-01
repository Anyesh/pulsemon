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
use collectors::worker::CollectorHandle;
use config::Config;
use event::{AppEvent, EventHandler};
use terminal::TerminalGuard;

fn main() -> Result<()> {
    let config = Config::parse();
    let mut guard = TerminalGuard::new()?;
    run(guard.terminal(), &config)
}

fn run(terminal: &mut ratatui::DefaultTerminal, config: &Config) -> Result<()> {
    let events = EventHandler::new();
    let collector = CollectorHandle::spawn(config, events.sender());
    let mut app = App::new(config, collector);
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
                AppEvent::Snapshot(snapshot) => app.apply_snapshot(*snapshot),
                AppEvent::Notice(msg) => app.set_status(msg),
            }
            dirty = true;
            next = events.try_recv()?;
        }

        if !app.running {
            return Ok(());
        }

        let now = Instant::now();
        if schedule::tick_due(now, last_tick, app.tick_rate) {
            app.request_refresh();
            last_tick = now;
        }
        dirty |= app.expire_status(now);
    }
}
