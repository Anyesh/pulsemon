use crate::types::GpuMetrics;
use anyhow::Result;

pub trait GpuBackend: Send {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn refresh(&mut self) -> Result<()>;
    fn metrics(&self) -> Vec<GpuMetrics>;
}

mod nvidia;

#[cfg(target_os = "linux")]
mod amd;
#[cfg(target_os = "macos")]
mod apple;
#[cfg(target_os = "linux")]
mod intel;

pub fn detect_gpus() -> Vec<Box<dyn GpuBackend>> {
    let mut backends: Vec<Box<dyn GpuBackend>> = Vec::new();

    // Try NVIDIA (cross-platform)
    if let Some(nvidia) = nvidia::NvidiaBackend::try_new() {
        backends.push(Box::new(nvidia));
    }

    // Try AMD (Linux only)
    #[cfg(target_os = "linux")]
    {
        if let Some(amd) = amd::AmdBackend::try_new() {
            backends.push(Box::new(amd));
        }
    }

    // Try Intel (Linux only)
    #[cfg(target_os = "linux")]
    {
        if let Some(intel) = intel::IntelBackend::try_new() {
            backends.push(Box::new(intel));
        }
    }

    // Try Apple Silicon (macOS only)
    #[cfg(target_os = "macos")]
    {
        if let Some(apple) = apple::AppleBackend::try_new() {
            backends.push(Box::new(apple));
        }
    }

    backends
}
