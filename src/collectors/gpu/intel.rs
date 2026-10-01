use super::GpuBackend;
use crate::types::GpuMetrics;
use anyhow::Result;
use std::process::Command;

pub struct IntelBackend {
    cached_metrics: Vec<GpuMetrics>,
}

impl IntelBackend {
    pub fn try_new() -> Option<Self> {
        // Check if intel_gpu_top is available
        let output = Command::new("which").arg("intel_gpu_top").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(Self {
            cached_metrics: Vec::new(),
        })
    }
}

impl GpuBackend for IntelBackend {
    fn refresh(&mut self) -> Result<()> {
        // intel_gpu_top -J -s 1000 -n 1 outputs one JSON sample
        let output = Command::new("intel_gpu_top")
            .args(["-J", "-s", "500", "-n", "1"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.cached_metrics = parse_intel_gpu_output(&stdout);
        Ok(())
    }

    fn metrics(&self) -> Vec<GpuMetrics> {
        self.cached_metrics.clone()
    }
}

fn parse_intel_gpu_output(output: &str) -> Vec<GpuMetrics> {
    // intel_gpu_top JSON output has "engines" with "busy" percentages
    let mut utilization = None;

    for line in output.lines() {
        let line = line.trim();
        if line.contains("\"busy\"") {
            if let Some(colon) = line.find(':') {
                let val_str = line[colon + 1..]
                    .trim()
                    .trim_matches(|c| c == ',' || c == ' ');
                if let Ok(val) = val_str.parse::<f32>() {
                    // Take the highest busy value as overall utilization
                    utilization = Some(utilization.map_or(val, |prev: f32| prev.max(val)));
                }
            }
        }
    }

    if utilization.is_some() {
        vec![GpuMetrics {
            name: String::from("Intel GPU"),
            utilization,
            memory_used: None,
            memory_total: None,
            temperature: None,
            power_usage: None,
            power_limit: None,
            fan_speed: None,
        }]
    } else {
        Vec::new()
    }
}
