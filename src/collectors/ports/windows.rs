use std::ffi::c_void;
use std::mem::size_of;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ptr;

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

use super::PortScanner;
use crate::types::PortInfo;

/// Reads the IP Helper owner-pid tables directly instead of spawning `netstat -ano`
/// on every refresh.
pub struct WindowsPortScanner;

impl PortScanner for WindowsPortScanner {
    fn scan(&mut self) -> Result<Vec<PortInfo>> {
        let mut ports = Vec::new();
        let tcp = |af: u16| {
            fetch_table(|buf, size| unsafe {
                GetExtendedTcpTable(buf, size, 0, af as u32, TCP_TABLE_OWNER_PID_ALL, 0)
            })
        };
        let udp = |af: u16| {
            fetch_table(|buf, size| unsafe {
                GetExtendedUdpTable(buf, size, 0, af as u32, UDP_TABLE_OWNER_PID, 0)
            })
        };

        for row in rows::<MIB_TCPROW_OWNER_PID>(&tcp(AF_INET)?) {
            ports.push(PortInfo {
                protocol: "TCP".into(),
                local_addr: ipv4(row.dwLocalAddr),
                local_port: port(row.dwLocalPort),
                remote_addr: ipv4(row.dwRemoteAddr),
                remote_port: port(row.dwRemotePort),
                state: tcp_state(row.dwState).into(),
                pid: Some(row.dwOwningPid),
                process_name: String::new(),
            });
        }
        for row in rows::<MIB_TCP6ROW_OWNER_PID>(&tcp(AF_INET6)?) {
            ports.push(PortInfo {
                protocol: "TCP".into(),
                local_addr: Ipv6Addr::from(row.ucLocalAddr).to_string(),
                local_port: port(row.dwLocalPort),
                remote_addr: Ipv6Addr::from(row.ucRemoteAddr).to_string(),
                remote_port: port(row.dwRemotePort),
                state: tcp_state(row.dwState).into(),
                pid: Some(row.dwOwningPid),
                process_name: String::new(),
            });
        }
        for row in rows::<MIB_UDPROW_OWNER_PID>(&udp(AF_INET)?) {
            ports.push(udp_port(
                ipv4(row.dwLocalAddr),
                row.dwLocalPort,
                row.dwOwningPid,
            ));
        }
        for row in rows::<MIB_UDP6ROW_OWNER_PID>(&udp(AF_INET6)?) {
            let addr = Ipv6Addr::from(row.ucLocalAddr).to_string();
            ports.push(udp_port(addr, row.dwLocalPort, row.dwOwningPid));
        }
        Ok(ports)
    }
}

fn udp_port(local_addr: String, local_port: u32, pid: u32) -> PortInfo {
    PortInfo {
        protocol: "UDP".into(),
        local_addr,
        local_port: port(local_port),
        remote_addr: "*".into(),
        remote_port: 0,
        state: String::new(),
        pid: Some(pid),
        process_name: String::new(),
    }
}

/// Calls an IP Helper table function, growing the buffer until the table fits. The
/// buffer is `u32`-backed because every table and row type is 4-byte aligned.
fn fetch_table(call: impl Fn(*mut c_void, *mut u32) -> u32) -> Result<Vec<u32>> {
    let mut size: u32 = 0;
    let mut buf: Vec<u32> = Vec::new();
    // The table can grow between the size query and the read, so retry a few times.
    for _ in 0..4 {
        let status = call(buf.as_mut_ptr().cast(), &mut size);
        match status {
            NO_ERROR => return Ok(buf),
            ERROR_INSUFFICIENT_BUFFER => buf = vec![0; (size as usize).div_ceil(4)],
            err => bail!("IP Helper table query failed with error {err}"),
        }
    }
    bail!("IP Helper table kept growing")
}

/// Rows of a `MIB_*TABLE_OWNER_PID`: a `u32` entry count followed by the rows.
fn rows<T: Copy>(table: &[u32]) -> Vec<T> {
    let Some(&count) = table.first() else {
        return Vec::new();
    };
    let available = (table.len() - 1) * 4 / size_of::<T>();
    let count = (count as usize).min(available);
    let base = table[1..].as_ptr().cast::<T>();
    // SAFETY: `count` is clamped to the rows that fit in the buffer, and the rows
    // start right after the 4-byte count, matching the table's C layout.
    (0..count)
        .map(|i| unsafe { ptr::read_unaligned(base.add(i)) })
        .collect()
}

/// Ports are stored in network byte order in the low 16 bits of a DWORD.
fn port(dword: u32) -> u16 {
    u16::from_be(dword as u16)
}

/// IPv4 addresses are stored in network byte order, so the in-memory bytes are the
/// address octets.
fn ipv4(dword: u32) -> String {
    Ipv4Addr::from(dword.to_ne_bytes()).to_string()
}

fn tcp_state(state: u32) -> &'static str {
    match state {
        1 => "CLOSED",
        2 => "LISTEN",
        3 => "SYN_SENT",
        4 => "SYN_RECV",
        5 => "ESTABLISHED",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "CLOSE_WAIT",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "TIME_WAIT",
        12 => "DELETE_TCB",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_network_order_fields() {
        let port_8080 = u32::from(8080u16.to_be());
        assert_eq!(port(port_8080), 8080);
        let loopback = u32::from_ne_bytes([127, 0, 0, 1]);
        assert_eq!(ipv4(loopback), "127.0.0.1");
    }

    #[test]
    fn reads_rows_after_count() {
        let row = MIB_UDPROW_OWNER_PID {
            dwLocalAddr: u32::from_ne_bytes([10, 0, 0, 1]),
            dwLocalPort: u32::from(53u16.to_be()),
            dwOwningPid: 1234,
        };
        let table = vec![1, row.dwLocalAddr, row.dwLocalPort, row.dwOwningPid];
        let parsed = rows::<MIB_UDPROW_OWNER_PID>(&table);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].dwOwningPid, 1234);
    }

    #[test]
    fn count_larger_than_buffer_is_clamped() {
        let table = vec![99, 1, 2, 3];
        assert_eq!(rows::<MIB_UDPROW_OWNER_PID>(&table).len(), 1);
        assert!(rows::<MIB_UDPROW_OWNER_PID>(&[]).is_empty());
    }
}
