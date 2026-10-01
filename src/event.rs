use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyEvent};

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize,
}

/// Reads terminal input on a background thread. Ticks are not produced here: the
/// main loop owns the tick deadline so that steady input cannot starve refreshes.
pub struct EventHandler {
    receiver: Receiver<AppEvent>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || loop {
            let Ok(event) = event::read() else { return };
            let forwarded = match event {
                Event::Key(key) => AppEvent::Key(key),
                Event::Resize(..) => AppEvent::Resize,
                _ => continue,
            };
            if sender.send(forwarded).is_err() {
                return;
            }
        });
        Self { receiver }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<AppEvent>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => bail!("input thread stopped"),
        }
    }

    pub fn try_recv(&self) -> Result<Option<AppEvent>> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("input thread stopped"),
        }
    }
}
