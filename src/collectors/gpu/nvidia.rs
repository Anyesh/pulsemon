use super::GpuBackend;
use crate::types::GpuMetrics;
use anyhow::Result;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;

pub struct NvidiaBackend {
    nvml: Nvml,
    device_count: u32,
    cached_metrics: Vec<GpuMetrics>,
}

impl NvidiaBackend {
    pub fn try_new() -> Option<Self> {
        let nvml = Nvml::init().ok()?;
        let device_count = nvml.device_count().ok()?;
        if device_count == 0 {
            return None;
        }

        let mut backend = Self {
            nvml,
            device_count,
            cached_metrics: Vec::new(),
        };

        // Do initial refresh
        let _ = backend.refresh();
        Some(backend)
    }
}

impl GpuBackend for NvidiaBackend {
    fn refresh(&mut self) -> Result<()> {
        let mut metrics = Vec::new();

        for idx in 0..self.device_count {
            let device = match self.nvml.device_by_index(idx) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let name = device.name().unwrap_or_else(|_| format!("GPU {}", idx));

            let utilization = device.utilization_rates().ok().map(|u| u.gpu as f32);

            let (memory_used, memory_total) = device
                .memory_info()
                .ok()
                .map(|m| (Some(m.used), Some(m.total)))
                .unwrap_or((None, None));

            let temperature = device
                .temperature(TemperatureSensor::Gpu)
                .ok()
                .map(|t| t as f32);

            let power_usage = device.power_usage().ok().map(|p| p as f32 / 1000.0);

            let power_limit = device
                .enforced_power_limit()
                .ok()
                .map(|p| p as f32 / 1000.0);

            let fan_speed = device.fan_speed(0).ok();

            metrics.push(GpuMetrics {
                name,
                utilization,
                memory_used,
                memory_total,
                temperature,
                power_usage,
                power_limit,
                fan_speed,
            });
        }

        self.cached_metrics = metrics;
        Ok(())
    }

    fn metrics(&self) -> Vec<GpuMetrics> {
        self.cached_metrics.clone()
    }
}
