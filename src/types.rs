use std::collections::VecDeque;
use std::sync::Arc;

const HISTORY_CAPACITY: usize = 60;

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CpuMetrics {
    pub global_usage: f32,
    pub per_core: Vec<f32>,
    pub cpu_name: String,
    pub history: VecDeque<f32>,
}

impl Default for CpuMetrics {
    fn default() -> Self {
        Self {
            global_usage: 0.0,
            per_core: Vec::new(),
            cpu_name: String::new(),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MemoryMetrics {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub history: VecDeque<f32>,
}

impl Default for MemoryMetrics {
    fn default() -> Self {
        Self {
            total: 0,
            used: 0,
            swap_total: 0,
            swap_used: 0,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub fs_type: String,
}

// ---------------------------------------------------------------------------
// Process
// ---------------------------------------------------------------------------

/// Identifies one process across its lifetime. A pid alone is not enough because
/// the OS reuses pids, so a table selection or a pending signal keyed by pid could
/// land on an unrelated process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcKey {
    pub pid: u32,
    pub start_time: u64,
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub key: ProcKey,
    pub parent: Option<u32>,
    pub name: String,
    pub name_lower: String,
    pub command: String,
    pub command_lower: String,
    pub user: Arc<str>,
    pub cpu_usage: f32,
    pub memory: u64,
    /// Bytes read plus written per second over the last refresh interval.
    pub disk_rate: u64,
    pub status: &'static str,
}

impl ProcessInfo {
    pub fn pid(&self) -> u32 {
        self.key.pid
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProcessSortBy {
    Pid,
    Name,
    #[default]
    Cpu,
    Memory,
}

// ---------------------------------------------------------------------------
// Port / Network
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
    pub pid: Option<u32>,
    pub process_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PortSortBy {
    #[default]
    Port,
    Protocol,
    Pid,
    State,
}

// ---------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GpuMetrics {
    pub name: String,
    pub utilization: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature: Option<f32>,
    pub power_usage: Option<f32>,
    pub power_limit: Option<f32>,
    pub fan_speed: Option<u32>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a byte count into a human-readable string (e.g. "1.23 GB").
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let value = bytes as f64;

    if value >= TB {
        format!("{:.2} TB", value / TB)
    } else if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.2} MB", value / MB)
    } else if value >= KB {
        format!("{:.2} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.00 TB");
    }

    #[test]
    fn test_cpu_metrics_default_history_capacity() {
        let cpu = CpuMetrics::default();
        assert_eq!(cpu.history.capacity(), HISTORY_CAPACITY);
        assert!(cpu.history.is_empty());
    }

    #[test]
    fn test_memory_metrics_default_history_capacity() {
        let mem = MemoryMetrics::default();
        assert_eq!(mem.history.capacity(), HISTORY_CAPACITY);
        assert!(mem.history.is_empty());
    }

    #[test]
    fn test_sort_defaults() {
        assert_eq!(ProcessSortBy::default(), ProcessSortBy::Cpu);
        assert_eq!(PortSortBy::default(), PortSortBy::Port);
    }
}
