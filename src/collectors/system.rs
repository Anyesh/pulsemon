use std::time::{Duration, Instant};

use sysinfo::{DiskRefreshKind, Disks, System};

use crate::types::{CpuMetrics, DiskInfo, MemoryMetrics};

const HISTORY_LEN: usize = 60;
const DISK_LIST_INTERVAL: Duration = Duration::from_secs(10);

pub struct SystemCollector {
    sys: System,
    disks: Disks,
    disks_listed_at: Instant,
    cpu_metrics: CpuMetrics,
    memory_metrics: MemoryMetrics,
    disk_metrics: Vec<DiskInfo>,
}

impl SystemCollector {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Perform an initial CPU refresh so the *next* call returns real data.
        sys.refresh_cpu_usage();
        let disks = Disks::new_with_refreshed_list_specifics(storage_only());

        let mut collector = Self {
            sys,
            disks,
            disks_listed_at: Instant::now(),
            cpu_metrics: CpuMetrics::default(),
            memory_metrics: MemoryMetrics::default(),
            disk_metrics: Vec::new(),
        };
        collector.rebuild_disk_metrics();
        collector
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.update_cpu();
        self.update_memory();
        self.refresh_disks();
    }

    pub fn cpu(&self) -> &CpuMetrics {
        &self.cpu_metrics
    }

    pub fn memory(&self) -> &MemoryMetrics {
        &self.memory_metrics
    }

    pub fn disks(&self) -> &[DiskInfo] {
        &self.disk_metrics
    }

    fn update_cpu(&mut self) {
        let cpus = self.sys.cpus();
        let metrics = &mut self.cpu_metrics;

        metrics.per_core.clear();
        metrics.per_core.extend(cpus.iter().map(|c| c.cpu_usage()));
        metrics.global_usage = if metrics.per_core.is_empty() {
            0.0
        } else {
            metrics.per_core.iter().sum::<f32>() / metrics.per_core.len() as f32
        };
        push_history(&mut metrics.history, metrics.global_usage);

        if metrics.cpu_name.is_empty() {
            if let Some(cpu) = cpus.first() {
                metrics.cpu_name = cpu.brand().to_string();
            }
        }
    }

    fn update_memory(&mut self) {
        let metrics = &mut self.memory_metrics;
        metrics.total = self.sys.total_memory();
        metrics.used = self.sys.used_memory();
        metrics.swap_total = self.sys.total_swap();
        metrics.swap_used = self.sys.used_swap();

        let usage_pct = if metrics.total > 0 {
            (metrics.used as f32 / metrics.total as f32) * 100.0
        } else {
            0.0
        };
        push_history(&mut metrics.history, usage_pct);
    }

    /// Re-reads the mount table only every few seconds; in between, each known disk
    /// just has its space figures refreshed.
    fn refresh_disks(&mut self) {
        if self.disks_listed_at.elapsed() >= DISK_LIST_INTERVAL {
            self.disks.refresh_specifics(true, storage_only());
            self.disks_listed_at = Instant::now();
            self.rebuild_disk_metrics();
            return;
        }
        for (disk, info) in self.disks.list_mut().iter_mut().zip(&mut self.disk_metrics) {
            disk.refresh_specifics(storage_only());
            info.total = disk.total_space();
            info.used = disk.total_space().saturating_sub(disk.available_space());
        }
    }

    fn rebuild_disk_metrics(&mut self) {
        self.disk_metrics = self
            .disks
            .list()
            .iter()
            .map(|d| DiskInfo {
                name: d.name().to_string_lossy().into_owned(),
                mount_point: d.mount_point().to_string_lossy().into_owned(),
                total: d.total_space(),
                used: d.total_space().saturating_sub(d.available_space()),
                fs_type: d.file_system().to_string_lossy().into_owned(),
            })
            .collect();
    }
}

fn storage_only() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage()
}

fn push_history(history: &mut std::collections::VecDeque<f32>, value: f32) {
    if history.len() >= HISTORY_LEN {
        history.pop_front();
    }
    history.push_back(value);
}
