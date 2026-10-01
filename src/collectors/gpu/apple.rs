use super::GpuBackend;
use crate::types::GpuMetrics;
use anyhow::Result;
use std::process::Command;

pub struct AppleBackend {
    gpu_name: String,
    cached_metrics: Vec<GpuMetrics>,
}

impl AppleBackend {
    pub fn try_new() -> Option<Self> {
        // Check if we can get GPU info from ioreg
        let output = Command::new("ioreg")
            .args(["-r", "-d", "1", "-c", "IOAccelerator"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let gpu_name = extract_gpu_name(&stdout).unwrap_or_else(|| String::from("Apple GPU"));

        Some(Self {
            gpu_name,
            cached_metrics: Vec::new(),
        })
    }
}

impl GpuBackend for AppleBackend {
    fn name(&self) -> &str {
        "Apple"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn refresh(&mut self) -> Result<()> {
        // Try to get GPU utilization from powermetrics (requires sudo)
        // Falls back to just showing the GPU name if powermetrics fails
        let utilization = get_gpu_utilization();

        self.cached_metrics = vec![GpuMetrics {
            name: self.gpu_name.clone(),
            utilization,
            memory_used: None, // Apple Silicon shares system memory
            memory_total: None,
            temperature: None,
            power_usage: None,
            power_limit: None,
            fan_speed: None,
        }];

        Ok(())
    }

    fn metrics(&self) -> Vec<GpuMetrics> {
        self.cached_metrics.clone()
    }
}

fn extract_gpu_name(ioreg_output: &str) -> Option<String> {
    for line in ioreg_output.lines() {
        if line.contains("\"model\"") {
            // Extract value from "model" = <"Apple M1 Pro">
            if let Some(start) = line.find('"') {
                let rest = &line[start + 1..];
                if let Some(eq_pos) = rest.find("= ") {
                    let value = &rest[eq_pos + 2..];
                    let value = value
                        .trim()
                        .trim_matches(|c| c == '"' || c == '<' || c == '>');
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }
    }
    None
}

fn get_gpu_utilization() -> Option<f32> {
    // powermetrics requires sudo, so this may fail for non-root users
    let output = Command::new("powermetrics")
        .args(["--samplers", "gpu_power", "-i", "1000", "-n", "1"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("GPU Active") || line.contains("GPU active") {
            // Extract percentage from line like "GPU Active: 42%"
            if let Some(pct_pos) = line.find('%') {
                let before = &line[..pct_pos];
                let num_start = before
                    .rfind(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(0)
                    + 1;
                if let Ok(val) = before[num_start..].parse::<f32>() {
                    return Some(val);
                }
            }
        }
    }

    None
}
