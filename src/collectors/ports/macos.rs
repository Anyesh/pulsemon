use std::process::Command;

use anyhow::Result;

use super::PortScanner;
use crate::types::PortInfo;

pub struct MacosPortScanner;

impl PortScanner for MacosPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        // lsof exits non-zero when some processes cannot be inspected, but still
        // prints everything it could read, so the status is not an error here.
        let output = Command::new("lsof").args(["-i", "-n", "-P"]).output()?;
        Ok(parse_lsof(&String::from_utf8_lossy(&output.stdout)))
    }
}

/// Parses `lsof -i -n -P`, whose columns are
/// COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME [(STATE)].
pub fn parse_lsof(output: &str) -> Vec<PortInfo> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                return None;
            }
            let (local, remote) = match parts[8].split_once("->") {
                Some((local, remote)) => (local, Some(remote)),
                None => (parts[8], None),
            };
            let (local_addr, local_port) = parse_addr_port(local);
            let (remote_addr, remote_port) =
                remote.map_or_else(|| ("*".to_string(), 0), parse_addr_port);
            let state = parts
                .get(9)
                .map(|s| s.trim_start_matches('(').trim_end_matches(')').to_string())
                .unwrap_or_default();
            Some(PortInfo {
                protocol: parts[7].to_string(),
                local_addr,
                local_port,
                remote_addr,
                remote_port,
                state,
                pid: parts[1].parse().ok(),
                process_name: parts[0].replace("\\x20", " "),
            })
        })
        .collect()
}

/// `127.0.0.1:80`, `*:80` or `[::1]:80`; lsof brackets IPv6 addresses.
fn parse_addr_port(s: &str) -> (String, u16) {
    match s.rsplit_once(':') {
        Some((addr, port)) => (
            addr.trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
            port.parse().unwrap_or(0),
        ),
        None => (s.to_string(), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSOF: &str = "\
COMMAND     PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
rapportd    512 me    4u  IPv4 0x1234567890abcdef      0t0  TCP *:49152 (LISTEN)
Google\\x20  901 me   23u  IPv6 0x2234567890abcdef      0t0  TCP [::1]:8080 (LISTEN)
curl       1777 me    5u  IPv4 0x3234567890abcdef      0t0  TCP 198.51.100.15:52341->203.0.113.7:443 (ESTABLISHED)
mDNSRespo   300 me    8u  IPv6 0x4234567890abcdef      0t0  UDP [fe80::1%lo0]:5353
";

    #[test]
    fn parses_listeners_connections_and_udp() {
        let ports = parse_lsof(LSOF);
        assert_eq!(ports.len(), 4);

        assert_eq!(ports[0].local_addr, "*");
        assert_eq!(ports[0].local_port, 49152);
        assert_eq!(ports[0].state, "LISTEN");
        assert_eq!(ports[0].pid, Some(512));

        assert_eq!(ports[1].local_addr, "::1");
        assert_eq!(ports[1].process_name, "Google ");

        assert_eq!(ports[2].remote_addr, "203.0.113.7");
        assert_eq!(ports[2].remote_port, 443);
        assert_eq!(ports[2].state, "ESTABLISHED");

        assert_eq!(ports[3].protocol, "UDP");
        assert_eq!(ports[3].local_addr, "fe80::1%lo0");
        assert_eq!(ports[3].state, "");
        assert_eq!(ports[3].remote_addr, "*");
    }

    #[test]
    fn skips_header_and_short_lines() {
        assert!(parse_lsof("COMMAND PID\ngarbage\n").is_empty());
    }
}
