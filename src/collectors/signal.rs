use std::fmt;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::types::ProcKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Term,
    Hup,
    Int,
    Stop,
    Cont,
    Kill,
}

impl Signal {
    pub fn name(self) -> &'static str {
        if cfg!(windows) {
            return "Terminate";
        }
        match self {
            Self::Term => "TERM",
            Self::Hup => "HUP",
            Self::Int => "INT",
            Self::Stop => "STOP",
            Self::Cont => "CONT",
            Self::Kill => "KILL",
        }
    }

    pub fn meaning(self) -> &'static str {
        if cfg!(windows) {
            return "end the process immediately";
        }
        match self {
            Self::Term => "ask it to exit",
            Self::Hup => "hang up, often reloads config",
            Self::Int => "interrupt, like Ctrl-C",
            Self::Stop => "pause it",
            Self::Cont => "resume a paused process",
            Self::Kill => "force exit, cannot be caught",
        }
    }
}

/// Signals the platform can deliver. sysinfo's `kill_with` returns `None` on Windows
/// for anything but `Kill` (TerminateProcess), so the menu offers only that there.
pub fn available() -> &'static [Signal] {
    if cfg!(windows) {
        &[Signal::Kill]
    } else {
        &[
            Signal::Term,
            Signal::Hup,
            Signal::Int,
            Signal::Stop,
            Signal::Cont,
            Signal::Kill,
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SignalError {
    Exited,
    /// The pid now belongs to a process started at a different time.
    Reused,
    Failed(String),
}

impl fmt::Display for SignalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exited => write!(f, "it has already exited"),
            Self::Reused => write!(f, "its pid now belongs to another process"),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// Sends only if the pid still belongs to the process the user chose.
/// `current_start` is the start time of whatever holds the pid now, looked up just
/// before this call; `send` runs only when it matches.
pub fn guarded<S>(key: ProcKey, current_start: Option<u64>, send: S) -> Result<(), SignalError>
where
    S: FnOnce() -> Result<(), String>,
{
    match current_start {
        None => Err(SignalError::Exited),
        Some(start) if start != key.start_time => Err(SignalError::Reused),
        Some(_) => send().map_err(SignalError::Failed),
    }
}

/// Delivers `signal` to `key`, refusing if the pid has been reused. Uses its own
/// `System` so the lookup does not disturb the shared one's CPU accounting.
pub fn send(sys: &mut System, key: ProcKey, signal: Signal) -> Result<(), SignalError> {
    let pid = Pid::from_u32(key.pid);
    #[cfg(target_os = "linux")]
    {
        // A pidfd pins the process that holds the pid when it is opened, so checking
        // the start time afterwards and signalling through the fd leaves no window
        // for the pid to be reused in between.
        if let Some(fd) = linux::PidFd::open(key.pid) {
            let current = start_time(sys, pid);
            return guarded(key, current, || fd.send(signal));
        }
    }
    let current = start_time(sys, pid);
    guarded(key, current, || {
        match sys
            .process(pid)
            .and_then(|p| p.kill_with(to_sysinfo(signal)))
        {
            Some(true) => Ok(()),
            Some(false) => Err("the system refused the signal".into()),
            None => Err("not supported on this platform".into()),
        }
    })
}

fn start_time(sys: &mut System, pid: Pid) -> Option<u64> {
    let kind = ProcessRefreshKind::nothing().without_tasks();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
    sys.process(pid).map(|p| p.start_time())
}

fn to_sysinfo(signal: Signal) -> sysinfo::Signal {
    match signal {
        Signal::Term => sysinfo::Signal::Term,
        Signal::Hup => sysinfo::Signal::Hangup,
        Signal::Int => sysinfo::Signal::Interrupt,
        Signal::Stop => sysinfo::Signal::Stop,
        Signal::Cont => sysinfo::Signal::Continue,
        Signal::Kill => sysinfo::Signal::Kill,
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io;

    use super::Signal;

    pub struct PidFd(libc::c_int);

    impl PidFd {
        /// `None` on kernels without pidfd_open (before 5.3) or when the process is gone.
        pub fn open(pid: u32) -> Option<Self> {
            // SAFETY: pidfd_open takes a pid and flags and returns a new fd or -1.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
            (fd >= 0).then_some(Self(fd as libc::c_int))
        }

        pub fn send(&self, signal: Signal) -> Result<(), String> {
            let sig = match signal {
                Signal::Term => libc::SIGTERM,
                Signal::Hup => libc::SIGHUP,
                Signal::Int => libc::SIGINT,
                Signal::Stop => libc::SIGSTOP,
                Signal::Cont => libc::SIGCONT,
                Signal::Kill => libc::SIGKILL,
            };
            // SAFETY: the fd is open for our lifetime; null siginfo and zero flags
            // match a plain kill(2).
            let rc = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.0,
                    sig,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error().to_string())
            }
        }
    }

    impl Drop for PidFd {
        fn drop(&mut self) {
            // SAFETY: we own this fd and close it once.
            unsafe { libc::close(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const KEY: ProcKey = ProcKey {
        pid: 4242,
        start_time: 1_700_000_000,
    };

    #[test]
    fn sends_when_the_pid_still_belongs_to_the_process() {
        let sent = Cell::new(false);
        let result = guarded(KEY, Some(KEY.start_time), || {
            sent.set(true);
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert!(sent.get());
    }

    #[test]
    fn refuses_a_reused_pid_without_sending() {
        let sent = Cell::new(false);
        let result = guarded(KEY, Some(KEY.start_time + 30), || {
            sent.set(true);
            Ok(())
        });
        assert_eq!(result, Err(SignalError::Reused));
        assert!(!sent.get());
    }

    #[test]
    fn reports_an_exited_process_without_sending() {
        let sent = Cell::new(false);
        let result = guarded(KEY, None, || {
            sent.set(true);
            Ok(())
        });
        assert_eq!(result, Err(SignalError::Exited));
        assert!(!sent.get());
    }

    #[test]
    fn passes_through_delivery_failures() {
        let result = guarded(KEY, Some(KEY.start_time), || Err("EPERM".into()));
        assert_eq!(result, Err(SignalError::Failed("EPERM".into())));
    }

    #[test]
    fn signals_a_real_child_and_refuses_a_stale_key() {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
            .args(if cfg!(windows) {
                &["/C", "timeout /T 30 /NOBREAK >NUL"][..]
            } else {
                &["30"][..]
            })
            .spawn()
            .expect("spawn child");
        let pid = child.id();
        let mut sys = System::new();
        let start = start_time(&mut sys, Pid::from_u32(pid)).expect("child visible");

        let stale = ProcKey {
            pid,
            start_time: start + 1,
        };
        assert_eq!(
            send(&mut sys, stale, Signal::Kill),
            Err(SignalError::Reused)
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "stale key must not signal"
        );

        assert_eq!(
            send(
                &mut sys,
                ProcKey {
                    pid,
                    start_time: start
                },
                Signal::Kill
            ),
            Ok(())
        );
        child.wait().unwrap();
    }
}
