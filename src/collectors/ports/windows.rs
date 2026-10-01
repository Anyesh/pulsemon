use super::PortScanner;
use crate::types::PortInfo;
use anyhow::Result;
use std::process::Command;

pub struct WindowsPortScanner;

impl PortScanner for WindowsPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        let output = Command::new("netstat").args(["-ano"]).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut ports = Vec::new();

        for line in stdout.lines().skip(4) {
            if let Some(info) = parse_netstat_line(line) {
                ports.push(info);
            }
        }

        Ok(ports)
    }
}

fn parse_netstat_line(line: &str) -> Option<PortInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let protocol = parts[0].to_string();
    if protocol != "TCP" && protocol != "UDP" {
        return None;
    }

    let (local_addr, local_port) = parse_address(parts[1])?;

    let (remote_addr, remote_port) = if protocol == "TCP" && parts.len() > 2 {
        parse_address(parts[2]).unwrap_or_default()
    } else {
        (String::from("*"), 0)
    };

    let (state, pid_str) = if protocol == "TCP" {
        let state = if parts.len() > 3 {
            parts[3].to_string()
        } else {
            String::new()
        };
        let pid_s = if parts.len() > 4 { parts[4] } else { "" };
        (state, pid_s)
    } else {
        (String::new(), if parts.len() > 3 { parts[3] } else { "" })
    };

    let pid = pid_str.parse::<u32>().ok();

    Some(PortInfo {
        protocol,
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        state,
        pid,
        process_name: String::new(),
    })
}

fn parse_address(addr: &str) -> Option<(String, u16)> {
    if let Some(last_colon) = addr.rfind(':') {
        let ip = &addr[..last_colon];
        let port = addr[last_colon + 1..].parse::<u16>().ok()?;
        Some((ip.to_string(), port))
    } else {
        None
    }
}
