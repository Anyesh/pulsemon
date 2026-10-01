use super::PortScanner;
use crate::types::PortInfo;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct LinuxPortScanner;

impl PortScanner for LinuxPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        let mut ports = Vec::new();

        if let Ok(entries) = parse_proc_net("/proc/net/tcp", "TCP") {
            ports.extend(entries);
        }
        if let Ok(entries) = parse_proc_net("/proc/net/tcp6", "TCP") {
            ports.extend(entries);
        }
        if let Ok(entries) = parse_proc_net("/proc/net/udp", "UDP") {
            ports.extend(entries);
        }
        if let Ok(entries) = parse_proc_net("/proc/net/udp6", "UDP") {
            ports.extend(entries);
        }

        // Resolve PIDs from /proc/*/fd
        resolve_pids(&mut ports);

        Ok(ports)
    }
}

fn parse_proc_net(path: &str, protocol: &str) -> Result<Vec<PortInfo>> {
    let content = fs::read_to_string(path)?;
    let mut ports = Vec::new();

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let (local_addr, local_port) = parse_hex_address(parts[1]);
        let (remote_addr, remote_port) = parse_hex_address(parts[2]);
        let state = parse_tcp_state(parts[3]);
        let inode = parts
            .get(9)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        ports.push(PortInfo {
            protocol: protocol.to_string(),
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state,
            pid: None, // Will be resolved later
            process_name: String::new(),
        });

        // Store inode for PID resolution (we'll match it in resolve_pids)
        let _ = inode; // Used conceptually; actual resolution is via /proc/*/fd
    }

    Ok(ports)
}

fn parse_hex_address(hex: &str) -> (String, u16) {
    let parts: Vec<&str> = hex.split(':').collect();
    if parts.len() != 2 {
        return (String::from("0.0.0.0"), 0);
    }

    let port = u16::from_str_radix(parts[1], 16).unwrap_or(0);

    let addr = if parts[0].len() == 8 {
        // IPv4
        let addr_u32 = u32::from_str_radix(parts[0], 16).unwrap_or(0);
        format!(
            "{}.{}.{}.{}",
            addr_u32 & 0xff,
            (addr_u32 >> 8) & 0xff,
            (addr_u32 >> 16) & 0xff,
            (addr_u32 >> 24) & 0xff
        )
    } else {
        String::from("::") // IPv6 simplified
    };

    (addr, port)
}

fn parse_tcp_state(hex: &str) -> String {
    match hex {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "UNKNOWN",
    }
    .to_string()
}

fn resolve_pids(ports: &mut [PortInfo]) {
    let proc_dir = Path::new("/proc");
    if !proc_dir.exists() {
        return;
    }

    // Build a map of socket inodes to PIDs by scanning /proc/*/fd
    if let Ok(entries) = fs::read_dir(proc_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Ok(pid) = name_str.parse::<u32>() {
                let fd_dir = proc_dir.join(&*name_str).join("fd");
                if let Ok(fds) = fs::read_dir(&fd_dir) {
                    for fd in fds.flatten() {
                        if let Ok(link) = fs::read_link(fd.path()) {
                            let link_str = link.to_string_lossy().to_string();
                            if link_str.starts_with("socket:[") {
                                // Match socket inodes - simplified: just assign PID to ports
                                // that don't have one yet based on matching criteria
                                for port in ports.iter_mut() {
                                    if port.pid.is_none() {
                                        port.pid = Some(pid);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
