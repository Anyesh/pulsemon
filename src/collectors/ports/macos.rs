use super::PortScanner;
use crate::types::PortInfo;
use anyhow::Result;
use std::process::Command;

pub struct MacosPortScanner;

impl PortScanner for MacosPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        let output = Command::new("lsof")
            .args(["-i", "-n", "-P", "-F", "pcnPtT"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_lsof_output(&stdout)
    }
}

fn parse_lsof_output(output: &str) -> Result<Vec<PortInfo>> {
    // Fallback: use simpler lsof output format
    let output = Command::new("lsof").args(["-i", "-n", "-P"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let process_name = parts[0].to_string();
        let pid = parts[1].parse::<u32>().ok();
        let protocol = parts[7].to_string(); // TCP or UDP

        let name_field = parts[8]; // e.g., "127.0.0.1:8080->127.0.0.1:52341"

        let (local_part, remote_part) = if let Some(arrow_pos) = name_field.find("->") {
            (&name_field[..arrow_pos], Some(&name_field[arrow_pos + 2..]))
        } else {
            (name_field, None)
        };

        let (local_addr, local_port) = parse_addr_port(local_part);
        let (remote_addr, remote_port) = remote_part
            .map(parse_addr_port)
            .unwrap_or((String::from("*"), 0));

        let state = if parts.len() > 9 {
            parts[9]
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_string()
        } else {
            String::new()
        };

        ports.push(PortInfo {
            protocol,
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state,
            pid,
            process_name,
        });
    }

    Ok(ports)
}

fn parse_addr_port(s: &str) -> (String, u16) {
    if let Some(last_colon) = s.rfind(':') {
        let addr = &s[..last_colon];
        let port = s[last_colon + 1..].parse::<u16>().unwrap_or(0);
        (addr.to_string(), port)
    } else {
        (s.to_string(), 0)
    }
}
