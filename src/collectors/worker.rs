use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::gpu::{self, GpuBackend};
use super::ports::{self, PortScanner};
use super::process::ProcessCollector;
use super::system::SystemCollector;
use crate::config::Config;
use crate::event::AppEvent;
use crate::types::{CpuMetrics, DiskInfo, GpuMetrics, MemoryMetrics, PortInfo, ProcessInfo};

pub enum Request {
    Refresh { ports: bool },
    Kill { pid: u32, label: String },
}

#[derive(Default)]
pub struct Snapshot {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    /// `None` when this refresh skipped the port scan; the previous list still holds.
    pub ports: Option<Vec<PortInfo>>,
    pub gpu: Vec<GpuMetrics>,
    pub cost: Duration,
}

/// Sends requests to the collector thread. Collection runs off the UI thread because
/// a full refresh costs tens of milliseconds (sysinfo alone reads several /proc
/// files per process), which would otherwise stall input handling every tick.
pub struct CollectorHandle {
    tx: Sender<Request>,
}

impl CollectorHandle {
    pub fn spawn(config: &Config, events: Sender<AppEvent>) -> Self {
        let (tx, rx) = mpsc::channel();
        let no_ports = config.no_ports;
        let no_gpu = config.no_gpu;
        thread::Builder::new()
            .name("collector".into())
            .spawn(move || {
                let worker = Worker {
                    system: SystemCollector::new(),
                    processes: ProcessCollector::new(),
                    port_scanner: (!no_ports).then(ports::create_scanner),
                    gpu_backends: if no_gpu {
                        Vec::new()
                    } else {
                        gpu::detect_gpus()
                    },
                    events,
                };
                worker.run(rx);
            })
            .expect("failed to spawn collector thread");
        Self { tx }
    }

    pub fn send(&self, request: Request) {
        // A send only fails once the collector thread has exited, and its panic
        // already ended the process through the panic hook.
        let _ = self.tx.send(request);
    }
}

struct Worker {
    system: SystemCollector,
    processes: ProcessCollector,
    port_scanner: Option<Box<dyn PortScanner>>,
    gpu_backends: Vec<Box<dyn GpuBackend>>,
    events: Sender<AppEvent>,
}

impl Worker {
    fn run(mut self, rx: Receiver<Request>) {
        while let Ok(first) = rx.recv() {
            // Fold everything queued into one refresh so a slow refresh cannot build a
            // backlog; other requests run first so the refresh reflects them.
            let mut refresh: Option<bool> = None;
            for request in std::iter::once(first).chain(rx.try_iter()) {
                let reply = match request {
                    Request::Refresh { ports } => {
                        refresh = Some(refresh.unwrap_or(false) | ports);
                        continue;
                    }
                    Request::Kill { pid, label } => {
                        AppEvent::Notice(match self.processes.kill_process(pid) {
                            Ok(()) => format!("Killed {label} (PID {pid})"),
                            Err(e) => format!("Failed to kill {label}: {e}"),
                        })
                    }
                };
                if self.events.send(reply).is_err() {
                    return;
                }
            }
            if let Some(ports) = refresh {
                let snapshot = Box::new(self.refresh(ports));
                if self.events.send(AppEvent::Snapshot(snapshot)).is_err() {
                    return;
                }
            }
        }
    }

    fn refresh(&mut self, scan_ports: bool) -> Snapshot {
        let started = Instant::now();
        self.system.refresh();
        self.processes.refresh();
        let ports = if scan_ports {
            self.port_scanner
                .as_mut()
                .and_then(|scanner| scanner.scan().ok())
                .map(|mut ports| {
                    fill_process_names(&mut ports, self.processes.rows());
                    ports
                })
        } else {
            None
        };
        for backend in &mut self.gpu_backends {
            let _ = backend.refresh();
        }

        Snapshot {
            cpu: self.system.cpu().clone(),
            memory: self.system.memory().clone(),
            disks: self.system.disks().to_vec(),
            processes: self.processes.rows().to_vec(),
            ports,
            gpu: self.gpu_backends.iter().flat_map(|b| b.metrics()).collect(),
            cost: started.elapsed(),
        }
    }
}

/// Scanners that only know the owning pid get the name from the process table.
fn fill_process_names(ports: &mut [PortInfo], processes: &[ProcessInfo]) {
    let names: HashMap<u32, &str> = processes.iter().map(|p| (p.pid(), &*p.name)).collect();
    for port in ports.iter_mut().filter(|p| p.process_name.is_empty()) {
        if let Some(name) = port.pid.and_then(|pid| names.get(&pid)) {
            port.process_name = name.to_string();
        }
    }
}
