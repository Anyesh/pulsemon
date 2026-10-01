use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "pulsemon", about = "Cross-platform system monitor TUI")]
pub struct Config {
    /// Refresh rate in milliseconds
    #[arg(short, long, default_value = "1000")]
    pub rate: u64,

    /// Disable GPU monitoring
    #[arg(long)]
    pub no_gpu: bool,

    /// Disable port monitoring
    #[arg(long)]
    pub no_ports: bool,

    /// Show how long each collection pass takes in the status bar
    #[arg(long)]
    pub debug_timing: bool,
}
