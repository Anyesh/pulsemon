use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};

/// Events that the TUI application can handle.
#[derive(Debug)]
pub enum AppEvent {
    /// A key press event from the terminal.
    Key(KeyEvent),
    /// A periodic tick used to drive UI updates.
    Tick,
    /// The terminal was resized.
    Resize,
}

/// Polls crossterm events on a background thread and forwards them as
/// [`AppEvent`] values over an `mpsc` channel.
pub struct EventHandler {
    receiver: mpsc::Receiver<AppEvent>,
    #[allow(dead_code)]
    handler: thread::JoinHandle<()>,
}

impl EventHandler {
    /// Creates a new `EventHandler` that polls for terminal events using the
    /// given `tick_rate` as the poll timeout.
    ///
    /// A background thread is spawned immediately. On each poll cycle:
    /// - If a key event is available it is sent as `AppEvent::Key`.
    /// - If a resize event is available it is sent as `AppEvent::Resize`.
    /// - Otherwise an `AppEvent::Tick` is sent.
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();

        let handler = thread::spawn(move || {
            loop {
                match event::poll(tick_rate) {
                    Ok(true) => {
                        match event::read() {
                            Ok(Event::Key(key_event)) => {
                                if sender.send(AppEvent::Key(key_event)).is_err() {
                                    return;
                                }
                            }
                            Ok(Event::Resize(..)) => {
                                if sender.send(AppEvent::Resize).is_err() {
                                    return;
                                }
                            }
                            // Ignore mouse and other event types.
                            Ok(_) => {}
                            Err(_) => return,
                        }
                    }
                    Ok(false) => {
                        // No event within the tick_rate window — emit a tick.
                        if sender.send(AppEvent::Tick).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        Self { receiver, handler }
    }

    /// Blocks until the next [`AppEvent`] is available and returns it.
    pub fn next(&self) -> Result<AppEvent> {
        let event = self.receiver.recv()?;
        Ok(event)
    }
}
