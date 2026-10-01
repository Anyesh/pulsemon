use super::GpuBackend;
use crate::types::GpuMetrics;
use anyhow::Result;
use std::process::Command;

pub struct AmdBackend {
    cached_metrics: Vec<GpuMetrics>,
}

impl AmdBackend {
    pub fn try_new() -> Option<Self> {
        // Check if rocm-smi is available
        let output = Command::new("rocm-smi").arg("--showid").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let mut backend = Self {
            cached_metrics: Vec::new(),
        };
        let _ = backend.refresh();
        Some(backend)
    }
}

impl GpuBackend for AmdBackend {
    fn refresh(&mut self) -> Result<()> {
        let output = Command::new("rocm-smi")
            .args([
                "--showuse",
                "--showmemuse",
                "--showtemp",
                "--showpower",
                "--json",
            ])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Try to parse JSON output from rocm-smi
        self.cached_metrics = parse_rocm_smi_output(&stdout);
        Ok(())
    }

    fn metrics(&self) -> Vec<GpuMetrics> {
        self.cached_metrics.clone()
    }
}

fn parse_rocm_smi_output(output: &str) -> Vec<GpuMetrics> {
    // rocm-smi --json outputs a JSON object with card keys like "card0", "card1"
    // Simple parsing without a JSON library
    let mut metrics = Vec::new();
    let mut current_name = String::from("AMD GPU");
    let mut utilization = None;
    let mut temperature = None;
    let mut memory_used = None;
    let mut memory_total = None;
    let mut power_usage = None;

    for line in output.lines() {
        let line = line.trim();
        if line.contains("\"GPU use (%)\"") || line.contains("\"GPU Usage (%)\"") {
            if let Some(val) = extract_number(line) {
                utilization = Some(val);
            }
        } else if line.contains("\"Temperature") && line.contains("edge") {
            if let Some(val) = extract_number(line) {
                temperature = Some(val);
            }
        } else if line.contains("\"GPU memory use (%)\"") {
            if let Some(val) = extract_number(line) {
                // This is percentage, not bytes
                utilization = utilization.or(Some(val));
            }
        } else if line.contains("\"Average Graphics Package Power\"")
            || line.contains("\"Current Socket Graphics Package Power\"")
        {
            if let Some(val) = extract_number(line) {
                power_usage = Some(val);
            }
        } else if line.contains("\"card") && line.contains("\":") {
            // New card section - if we have data, push previous
            if utilization.is_some() || temperature.is_some() {
                metrics.push(GpuMetrics {
                    name: current_name.clone(),
                    utilization,
                    memory_used,
                    memory_total,
                    temperature,
                    power_usage,
                    power_limit: None,
                    fan_speed: None,
                });
            }
            current_name = format!("AMD GPU {}", metrics.len());
            utilization = None;
            temperature = None;
            memory_used = None;
            memory_total = None;
            power_usage = None;
        }
    }

    // Push last card
    if utilization.is_some() || temperature.is_some() {
        metrics.push(GpuMetrics {
            name: current_name,
            utilization,
            memory_used,
            memory_total,
            temperature,
            power_usage,
            power_limit: None,
            fan_speed: None,
        });
    }

    metrics
}

fn extract_number(line: &str) -> Option<f32> {
    // Find a number value in a JSON-like line: "key": "42.0"  or "key": 42.0
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let value = parts[1]
        .trim()
        .trim_matches(|c| c == '"' || c == ',' || c == ' ');
    value.parse::<f32>().ok()
}
