use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::Result;

use super::PortScanner;
use crate::types::PortInfo;

pub const TABLES: [(&str, &str); 4] = [
    ("tcp", "TCP"),
    ("tcp6", "TCP"),
    ("udp", "UDP"),
    ("udp6", "UDP"),
];

pub struct LinuxPortScanner;

impl PortScanner for LinuxPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        let mut sockets = Vec::new();
        for (file, protocol) in TABLES {
            if let Ok(content) = fs::read_to_string(format!("/proc/net/{file}")) {
                sockets.extend(parse_proc_net(&content, protocol));
            }
        }
        let owners = socket_owners();
        Ok(sockets
            .into_iter()
            .map(|(mut port, inode)| {
                port.pid = owners.get(&inode).copied();
                port
            })
            .collect())
    }
}

/// Parses one `/proc/net/{tcp,udp}[6]` table into sockets paired with their inode.
pub fn parse_proc_net(content: &str, protocol: &str) -> Vec<(PortInfo, u64)> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (local_addr, local_port) = parse_hex_address(fields.get(1)?)?;
            let (remote_addr, remote_port) = parse_hex_address(fields.get(2)?)?;
            let state = state_name(fields.get(3)?, protocol);
            let inode = fields.get(9).and_then(|s| s.parse().ok()).unwrap_or(0);
            let port = PortInfo {
                protocol: protocol.to_string(),
                local_addr,
                local_port,
                remote_addr,
                remote_port,
                state: state.to_string(),
                pid: None,
                process_name: String::new(),
            };
            Some((port, inode))
        })
        .collect()
}

/// The kernel prints each 32-bit word of the address with `%08X` from memory, so
/// the bytes inside every word come out in native order.
pub fn parse_hex_address(hex: &str) -> Option<(String, u16)> {
    let (addr, port) = hex.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let word = |i: usize| -> Option<[u8; 4]> {
        let chunk = addr.get(i * 8..i * 8 + 8)?;
        Some(u32::from_str_radix(chunk, 16).ok()?.to_ne_bytes())
    };
    let addr = match addr.len() {
        8 => Ipv4Addr::from(word(0)?).to_string(),
        32 => {
            let mut bytes = [0u8; 16];
            for i in 0..4 {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&word(i)?);
            }
            Ipv6Addr::from(bytes).to_string()
        }
        _ => return None,
    };
    Some((addr, port))
}

fn state_name(hex: &str, protocol: &str) -> &'static str {
    if protocol == "UDP" {
        // UDP has no connection states; the kernel reports bound sockets as CLOSE.
        return if hex == "01" { "ESTABLISHED" } else { "UNCONN" };
    }
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
}

/// `socket:[12345]` -> 12345
pub fn parse_socket_link(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Maps socket inodes to the pid holding them, by reading every `/proc/<pid>/fd`.
/// Only processes we may inspect are visible, so other users' sockets stay unowned
/// unless running as root.
fn socket_owners() -> HashMap<u64, u32> {
    let mut owners = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return owners;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        // Unreadable fd directories (other users' processes) just leave those
        // sockets without an owner.
        if let Ok(inodes) = pid_socket_inodes(pid) {
            owners.extend(inodes.into_iter().map(|inode| (inode, pid)));
        }
    }
    owners
}

pub fn pid_socket_inodes(pid: u32) -> std::io::Result<Vec<u64>> {
    let fds = fs::read_dir(format!("/proc/{pid}/fd"))?;
    Ok(fds
        .flatten()
        .filter_map(|fd| fs::read_link(fd.path()).ok())
        .filter_map(|link| link.to_str().and_then(parse_socket_link))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 52981 1 0000000000000000 100 0 0 10 0
   1: 0F6433C6:A3E2 077100CB:01BB 01 00000000:00000000 02:000A2B6F 00000000  1000        0 61234 2 0000000000000000 20 4 30 10 -1
   2: 0F6433C6:A3E4 077100CB:01BB 06 00000000:00000000 03:00000F2C 00000000     0        0 0 3 0000000000000000
";

    const TCP6: &str = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 24680 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 13579 1 0000000000000000 100 0 0 10 0
   2: 0000000000000000FFFF00000100007F:1F90 0000000000000000FFFF00000100007F:D431 01 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 100 0 0 10 0
   3: B80D01200000000000000000EFBEADDE:0050 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn parses_ipv4_rows_with_inodes() {
        let rows = parse_proc_net(TCP, "TCP");
        assert_eq!(rows.len(), 3);
        let (listen, inode) = &rows[0];
        assert_eq!(
            (listen.local_addr.as_str(), listen.local_port),
            ("127.0.0.1", 3306)
        );
        assert_eq!(listen.state, "LISTEN");
        assert_eq!(*inode, 52981);
        let (conn, _) = &rows[1];
        assert_eq!(conn.local_addr, "198.51.100.15");
        assert_eq!(
            (conn.remote_addr.as_str(), conn.remote_port),
            ("203.0.113.7", 443)
        );
        assert_eq!(conn.state, "ESTABLISHED");
        assert_eq!(rows[2].1, 0, "TIME_WAIT sockets have no inode");
    }

    #[test]
    fn formats_ipv6_addresses() {
        let rows = parse_proc_net(TCP6, "TCP");
        let addrs: Vec<&str> = rows.iter().map(|(p, _)| p.local_addr.as_str()).collect();
        assert_eq!(
            addrs,
            ["::", "::1", "::ffff:127.0.0.1", "2001:db8::dead:beef"]
        );
        assert_eq!(rows[0].0.local_port, 22);
        assert_eq!(rows[2].0.remote_port, 54321);
    }

    #[test]
    fn udp_sockets_are_unconnected_not_closed() {
        let udp = "  sl  local_address rem_address   st\n   0: 00000000:0044 00000000:0000 07 00000000:00000000 00:00000000 00000000     0        0 3456 2\n";
        let rows = parse_proc_net(udp, "UDP");
        assert_eq!(rows[0].0.state, "UNCONN");
        assert_eq!(rows[0].0.local_port, 68);
    }

    #[test]
    fn rejects_malformed_addresses() {
        assert_eq!(parse_hex_address("0100007F"), None);
        assert_eq!(parse_hex_address("XYZ:0016"), None);
        assert_eq!(parse_hex_address("0100007F:GGGG"), None);
    }

    #[test]
    fn reads_socket_inode_from_fd_link() {
        assert_eq!(parse_socket_link("socket:[52981]"), Some(52981));
        assert_eq!(parse_socket_link("pipe:[52981]"), None);
        assert_eq!(parse_socket_link("/dev/null"), None);
        assert_eq!(parse_socket_link("socket:[]"), None);
    }
}
