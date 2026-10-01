use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyEvent, MouseEvent, MouseEventKind};

use crate::collectors::inspect::ProcessDetail;
use crate::collectors::worker::Snapshot;

pub enum AppEvent {
    Key(KeyEvent),
    /// Stamped when read, so double-click timing is not skewed by a busy main loop.
    Mouse(MouseEvent, Instant),
    Resize,
    Snapshot(Box<Snapshot>),
    /// Inspector detail sent as soon as a new process is targeted, ahead of the next
    /// snapshot.
    Detail(Box<ProcessDetail>),
    /// A message for the status bar from the collector thread, such as a kill result.
    Notice(String),
}

/// Merges terminal input (read on a background thread) with collector results. Ticks
/// are not produced here: the main loop owns the tick deadline so that steady input
/// cannot starve refreshes.
pub struct EventHandler {
    sender: Sender<AppEvent>,
    receiver: Receiver<AppEvent>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let input = sender.clone();
        thread::spawn(move || loop {
            let Ok(event) = event::read() else { return };
            let forwarded = match event {
                Event::Key(key) => AppEvent::Key(key),
                // Motion and drags carry nothing we act on; Windows reports motion even
                // without motion tracking, so drop it before it reaches the queue.
                Event::Mouse(m)
                    if matches!(m.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) =>
                {
                    continue
                }
                Event::Mouse(m) => AppEvent::Mouse(m, Instant::now()),
                Event::Resize(..) => AppEvent::Resize,
                _ => continue,
            };
            if input.send(forwarded).is_err() {
                return;
            }
        });
        Self { sender, receiver }
    }

    /// For other producers, such as the collector thread, to feed the same queue.
    pub fn sender(&self) -> Sender<AppEvent> {
        self.sender.clone()
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
