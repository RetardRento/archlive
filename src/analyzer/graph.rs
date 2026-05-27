use chrono::{DateTime, Utc};
use crate::collector::Runtime;

/// A service node in the architecture graph
#[derive(Debug, Clone)]
pub struct Service {
    pub pid: u32,
    pub name: String,
    pub runtime: Runtime,
    pub script: Option<String>,
    pub cwd: Option<String>,
    pub ports: Vec<u16>,
}

impl Service {
    pub fn is_node_or_bun(&self) -> bool {
        matches!(self.runtime, Runtime::Node | Runtime::Bun)
    }

    pub fn runtime_badge(&self) -> &str {
        match self.runtime {
            Runtime::Node => "[node]",
            Runtime::Bun => "[bun] ",
            Runtime::Other(_) => "[sys] ",
        }
    }

    pub fn ports_display(&self) -> String {
        if self.ports.is_empty() {
            return "-".to_string();
        }
        self.ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A directed edge: one observed call path between two services
#[derive(Debug, Clone)]
pub struct Edge {
    pub src_pid: u32,
    pub dst_pid: u32,
    /// Total new TCP connections observed on this edge (proxy for HTTP requests)
    pub total_calls: u64,
    /// Calls observed in the last rolling window (for req/s)
    pub recent_calls: u64,
    /// Rolling window duration in seconds used to compute rate
    pub window_secs: u64,
    /// Seconds since this edge was last active
    pub idle_secs: u64,
}

impl Edge {
    pub fn req_per_sec(&self) -> f64 {
        if self.window_secs == 0 { return 0.0; }
        self.recent_calls as f64 / self.window_secs as f64
    }

    pub fn rate_label(&self) -> String {
        let rps = self.req_per_sec();
        if rps < 0.1 { "< 0.1/s".to_string() } else { format!("{:.1}/s", rps) }
    }

    pub fn idle_label(&self) -> String {
        match self.idle_secs {
            0 => "now".to_string(),
            1 => "1s ago".to_string(),
            s if s < 60 => format!("{}s ago", s),
            s => format!("{}m ago", s / 60),
        }
    }
}

/// Per-port inbound request statistics.
/// Tracks connections arriving at a listening port — each new TCP connection
/// to the port counts as one inbound HTTP request (good approximation for dev).
#[derive(Debug, Clone)]
pub struct PortStat {
    pub port: u16,
    pub pid: u32,
    pub service_name: String,
    /// Total inbound connections seen since the port was first detected
    pub total_requests: u64,
    /// Requests within the last rolling window
    pub recent_requests: u64,
    pub window_secs: u64,
    /// Currently open connections to this port
    pub active_connections: u64,
    /// Seconds since the last inbound connection
    pub idle_secs: u64,
}

impl PortStat {
    pub fn req_per_sec(&self) -> f64 {
        if self.window_secs == 0 { return 0.0; }
        self.recent_requests as f64 / self.window_secs as f64
    }

    pub fn rate_label(&self) -> String {
        let rps = self.req_per_sec();
        if rps < 0.1 { "< 0.1/s".to_string() } else { format!("{:.1}/s", rps) }
    }

    pub fn idle_label(&self) -> String {
        match self.idle_secs {
            0 => "now".to_string(),
            s if s < 60 => format!("{}s ago", s),
            s => format!("{}m ago", s / 60),
        }
    }
}

/// A single HTTP request captured by the tap proxy
#[derive(Debug, Clone)]
pub struct HttpEntry {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

impl HttpEntry {
    pub fn status_style_hint(&self) -> StatusKind {
        match self.status {
            200..=299 => StatusKind::Ok,
            300..=399 => StatusKind::Redirect,
            400..=499 => StatusKind::ClientError,
            500..=599 => StatusKind::ServerError,
            _ => StatusKind::Other,
        }
    }

    pub fn duration_label(&self) -> String {
        if self.duration_ms < 1000 {
            format!("{}ms", self.duration_ms)
        } else {
            format!("{:.1}s", self.duration_ms as f64 / 1000.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusKind { Ok, Redirect, ClientError, ServerError, Other }

/// Per-path aggregate stats from the tap
#[derive(Debug, Clone)]
pub struct PathStat {
    pub path: String,
    pub total_calls: u64,
    pub recent_calls: u64,   // last 10s window
    pub window_secs: u64,
    pub avg_duration_ms: u64,
    pub last_status: u16,
    pub error_count: u64,    // 4xx + 5xx
}

impl PathStat {
    pub fn req_per_sec(&self) -> f64 {
        if self.window_secs == 0 { return 0.0; }
        self.recent_calls as f64 / self.window_secs as f64
    }

    pub fn rate_label(&self) -> String {
        let rps = self.req_per_sec();
        if rps < 0.1 { "< 0.1/s".to_string() } else { format!("{:.1}/s", rps) }
    }
}

/// Point-in-time snapshot of the full graph, sent to the renderer each tick
#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    pub services: Vec<Service>,
    pub edges: Vec<Edge>,
    /// Inbound request stats for every port a tracked service listens on
    pub port_stats: Vec<PortStat>,
    /// Most recent HTTP requests from the tap (newest last, capped at 200)
    pub http_log: Vec<HttpEntry>,
    /// Per-path aggregates from the tap
    pub path_stats: Vec<PathStat>,
    pub tap_listen_port: Option<u16>,
    pub timestamp: DateTime<Utc>,
}

impl GraphSnapshot {
    pub fn node_bun_count(&self) -> usize {
        self.services.iter().filter(|s| s.is_node_or_bun()).count()
    }

    pub fn total_calls(&self) -> u64 {
        self.edges.iter().map(|e| e.total_calls).sum()
    }

    pub fn total_inbound(&self) -> u64 {
        self.port_stats.iter().map(|p| p.total_requests).sum()
    }

    pub fn tap_active(&self) -> bool {
        self.tap_listen_port.is_some()
    }

    pub fn tap_total_requests(&self) -> u64 {
        self.path_stats.iter().map(|p| p.total_calls).sum()
    }

    /// Path stats sorted by total calls descending
    pub fn sorted_path_stats(&self) -> Vec<&PathStat> {
        let mut v: Vec<&PathStat> = self.path_stats.iter().collect();
        v.sort_by(|a, b| b.total_calls.cmp(&a.total_calls));
        v
    }

    /// Sorted edge list for table display, highest call count first
    pub fn sorted_edges<'a>(&'a self, node_only: bool) -> Vec<(&'a Edge, &'a Service, &'a Service)> {
        let service_map: std::collections::HashMap<u32, &Service> =
            self.services.iter().map(|s| (s.pid, s)).collect();

        let mut rows: Vec<(&Edge, &Service, &Service)> = self
            .edges
            .iter()
            .filter_map(|e| {
                let src = service_map.get(&e.src_pid)?;
                let dst = service_map.get(&e.dst_pid)?;
                if node_only && !src.is_node_or_bun() && !dst.is_node_or_bun() {
                    return None;
                }
                Some((e, *src, *dst))
            })
            .collect();

        rows.sort_by(|a, b| b.0.total_calls.cmp(&a.0.total_calls));
        rows
    }

    /// Port stats sorted by total requests descending
    pub fn sorted_port_stats(&self) -> Vec<&PortStat> {
        let mut v: Vec<&PortStat> = self.port_stats.iter().collect();
        v.sort_by(|a, b| b.total_requests.cmp(&a.total_requests));
        v
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
