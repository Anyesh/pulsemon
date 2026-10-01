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

/// One row of the process table. The string fields are `Arc<str>` so that sending a
/// snapshot from the collector thread copies pointers rather than strings.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub key: ProcKey,
    pub parent: Option<u32>,
    pub name: Arc<str>,
    pub name_lower: Arc<str>,
    pub command: Arc<str>,
    pub command_lower: Arc<str>,
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

/// `YYYY-MM-DD HH:MM:SS UTC` for seconds since the Unix epoch.
pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Compact elapsed time: `45s`, `12m 05s`, `3h 02m`, `5d 04h`.
pub fn format_elapsed(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        secs % 86_400 / 3600,
        secs % 3600 / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h:02}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
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
    fn formats_epoch_as_utc() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(format_utc(1_790_000_000), "2026-09-21 14:13:20 UTC");
    }

    #[test]
    fn formats_elapsed_compactly() {
        assert_eq!(format_elapsed(45), "45s");
        assert_eq!(format_elapsed(725), "12m 05s");
        assert_eq!(format_elapsed(3 * 3600 + 120), "3h 02m");
        assert_eq!(format_elapsed(5 * 86400 + 4 * 3600), "5d 04h");
    }
}
