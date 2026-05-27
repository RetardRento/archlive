use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr};

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::net::Ipv6Addr;

/// State of a TCP connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    Listen,
    Established,
    TimeWait,
    CloseWait,
    Other(u8),
}

impl ConnState {
    fn from_code(code: u8) -> Self {
        match code {
            0x01 => ConnState::Established,
            0x0A => ConnState::Listen,
            0x06 => ConnState::TimeWait,
            0x08 => ConnState::CloseWait,
            other => ConnState::Other(other),
        }
    }
}

/// A single TCP connection (or listening socket) entry
#[derive(Debug, Clone)]
pub struct SocketConn {
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: IpAddr,
    pub remote_port: u16,
    pub state: ConnState,
    /// Socket inode — used to map this connection back to a PID
    pub inode: u64,
}

impl SocketConn {
    pub fn is_listen(&self) -> bool {
        self.state == ConnState::Listen
    }
    pub fn is_established(&self) -> bool {
        self.state == ConnState::Established
    }
}

/// Scan active TCP connections. On Linux reads /proc/net/tcp and /proc/net/tcp6.
/// On macOS falls back to `netstat`.
pub fn scan_connections(port_filter: Option<u16>) -> Result<Vec<SocketConn>> {
    #[cfg(target_os = "linux")]
    {
        scan_linux(port_filter)
    }
    #[cfg(target_os = "macos")]
    {
        scan_macos(port_filter)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(vec![])
    }
}

// ── Linux /proc/net/tcp ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn scan_linux(port_filter: Option<u16>) -> Result<Vec<SocketConn>> {
    let mut conns = Vec::new();
    parse_proc_net_tcp("/proc/net/tcp", false, port_filter, &mut conns)?;
    parse_proc_net_tcp("/proc/net/tcp6", true, port_filter, &mut conns)?;
    Ok(conns)
}

#[cfg(target_os = "linux")]
fn parse_proc_net_tcp(
    path: &str,
    is_ipv6: bool,
    port_filter: Option<u16>,
    out: &mut Vec<SocketConn>,
) -> Result<()> {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Ok(()), // file may not exist (e.g. no IPv6)
    };

    for line in content.lines().skip(1) {
        if let Some(conn) = parse_tcp_line(line, is_ipv6) {
            if let Some(pf) = port_filter {
                if conn.local_port != pf && conn.remote_port != pf {
                    continue;
                }
            }
            out.push(conn);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn parse_tcp_line(line: &str, is_ipv6: bool) -> Option<SocketConn> {
    // Format: sl  local_address rem_address   st tx_queue:rx_queue tr:tm->when retrnsmt  uid  timeout inode
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 10 {
        return None;
    }

    let local = parse_hex_addr(cols[1], is_ipv6)?;
    let remote = parse_hex_addr(cols[2], is_ipv6)?;
    let state_byte = u8::from_str_radix(cols[3], 16).ok()?;
    let inode: u64 = cols[9].parse().ok()?;

    Some(SocketConn {
        local_addr: local.0,
        local_port: local.1,
        remote_addr: remote.0,
        remote_port: remote.1,
        state: ConnState::from_code(state_byte),
        inode,
    })
}

/// Parse a hex-encoded address:port pair from /proc/net/tcp.
/// Linux stores addresses in little-endian hex.
#[cfg(target_os = "linux")]
fn parse_hex_addr(s: &str, is_ipv6: bool) -> Option<(IpAddr, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;

    if is_ipv6 {
        if addr_hex.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 16];
        for (i, chunk) in addr_hex.as_bytes().chunks(8).enumerate() {
            let word_str = std::str::from_utf8(chunk).ok()?;
            let word = u32::from_str_radix(word_str, 16).ok()?.to_le_bytes();
            bytes[i * 4..i * 4 + 4].copy_from_slice(&word);
        }
        Some((IpAddr::V6(Ipv6Addr::from(bytes)), port))
    } else {
        if addr_hex.len() != 8 {
            return None;
        }
        let word = u32::from_str_radix(addr_hex, 16).ok()?.to_le();
        Some((IpAddr::V4(Ipv4Addr::from(word)), port))
    }
}

// ── macOS netstat scanning ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn scan_macos(port_filter: Option<u16>) -> Result<Vec<SocketConn>> {
    use std::process::Command;

    // -n: no DNS, -f inet: IPv4, -p tcp: TCP only
    let out = Command::new("netstat")
        .args(["-an", "-p", "tcp"])
        .output()
        .context("run netstat")?;

    let text = String::from_utf8_lossy(&out.stdout);
    let mut conns = Vec::new();

    for line in text.lines() {
        if let Some(conn) = parse_netstat_line(line) {
            if let Some(pf) = port_filter {
                if conn.local_port != pf && conn.remote_port != pf {
                    continue;
                }
            }
            conns.push(conn);
        }
    }

    Ok(conns)
}

#[cfg(target_os = "macos")]
fn parse_netstat_line(line: &str) -> Option<SocketConn> {
    // Typical line: tcp4  0  0  127.0.0.1.3000  *.*  LISTEN
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 6 {
        return None;
    }
    if !cols[0].starts_with("tcp") {
        return None;
    }

    let local = parse_macos_addr(cols[3])?;
    let remote = parse_macos_addr_maybe_wildcard(cols[4]);
    let state_str = cols.last()?;
    let state = match *state_str {
        "LISTEN" => ConnState::Listen,
        "ESTABLISHED" => ConnState::Established,
        "TIME_WAIT" => ConnState::TimeWait,
        "CLOSE_WAIT" => ConnState::CloseWait,
        _ => ConnState::Other(0),
    };

    let (remote_addr, remote_port) = remote.unwrap_or((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));

    Some(SocketConn {
        local_addr: local.0,
        local_port: local.1,
        remote_addr,
        remote_port,
        state,
        inode: 0, // macOS netstat doesn't give inodes
    })
}

#[cfg(target_os = "macos")]
fn parse_macos_addr(s: &str) -> Option<(IpAddr, u16)> {
    // Format: "127.0.0.1.3000" or "*.3000" or "[::1].3000"
    let last_dot = s.rfind('.')?;
    let port: u16 = s[last_dot + 1..].parse().ok()?;
    let addr_str = &s[..last_dot];
    let addr = if addr_str == "*" || addr_str == "0.0.0.0" {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        addr_str.parse().ok()?
    };
    Some((addr, port))
}

#[cfg(target_os = "macos")]
fn parse_macos_addr_maybe_wildcard(s: &str) -> Option<(IpAddr, u16)> {
    if s == "*.*" {
        return Some((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
    }
    parse_macos_addr(s)
}
