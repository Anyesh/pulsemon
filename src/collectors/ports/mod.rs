use crate::types::PortInfo;
use anyhow::Result;

pub trait PortScanner: Send {
    fn scan(&mut self) -> Result<Vec<PortInfo>>;
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub fn create_scanner() -> Box<dyn PortScanner> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPortScanner)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxPortScanner)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosPortScanner)
    }
}
