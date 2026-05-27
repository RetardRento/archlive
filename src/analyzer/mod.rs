pub mod graph;

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::collector::{CollectorEvent, ProcessInfo, SocketConn};
use crate::collector::node_bun;
pub use graph::{Service, Edge, GraphSnapshot, PortStat, HttpEntry, PathStat};

const EDGE_TTL: Duration = Duration::from_secs(60);
const RATE_WINDOW: Duration = Duration::from_secs(10);

/// Identifies a unique TCP connection by its port pair.
/// remote_port is the client's ephemeral port, making each connection unique.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConnKey {
    local_port: u16,
    remote_port: u16,
}

struct CallEvent {
    at: Instant,
}

struct EdgeState {
    total_calls: u64,
    recent: VecDeque<CallEvent>,
    last_seen: Instant,
    first_seen: Instant,
}

impl EdgeState {
    fn new(now: Instant) -> Self {
        Self { total_calls: 0, recent: VecDeque::new(), last_seen: now, first_seen: now }
    }

    fn record_call(&mut self, now: Instant) {
        self.total_calls += 1;
        self.last_seen = now;
        self.recent.push_back(CallEvent { at: now });
    }

    fn evict_old_events(&mut self, now: Instant) {
        let cutoff = now - RATE_WINDOW;
        while self.recent.front().map(|e| e.at < cutoff).unwrap_or(false) {
            self.recent.pop_front();
        }
    }

    fn recent_calls(&self) -> u64 { self.recent.len() as u64 }
}

/// Per-port inbound request tracker
struct PortState {
    pid: u32,
    total_requests: u64,
    recent: VecDeque<CallEvent>,
    last_seen: Instant,
    /// How many ESTABLISHED connections currently point at this port as local_port
    active_connections: u64,
}

impl PortState {
    fn new(pid: u32, now: Instant) -> Self {
        Self { pid, total_requests: 0, recent: VecDeque::new(), last_seen: now, active_connections: 0 }
    }

    fn record_request(&mut self, now: Instant) {
        self.total_requests += 1;
        self.last_seen = now;
        self.recent.push_back(CallEvent { at: now });
    }

    fn evict_old_events(&mut self, now: Instant) {
        let cutoff = now - RATE_WINDOW;
        while self.recent.front().map(|e| e.at < cutoff).unwrap_or(false) {
            self.recent.pop_front();
        }
    }

    fn recent_requests(&self) -> u64 { self.recent.len() as u64 }
}

/// Per-path HTTP stats tracked by the tap
struct PathState {
    total_calls: u64,
    recent: VecDeque<CallEvent>,
    duration_sum_ms: u64,
    last_status: u16,
    error_count: u64,
}

impl PathState {
    fn new() -> Self {
        Self {
            total_calls: 0,
            recent: VecDeque::new(),
            duration_sum_ms: 0,
            last_status: 0,
            error_count: 0,
        }
    }

    fn record(&mut self, now: Instant, status: u16, duration_ms: u64) {
        self.total_calls += 1;
        self.duration_sum_ms += duration_ms;
        self.last_status = status;
        if status >= 400 { self.error_count += 1; }
        self.recent.push_back(CallEvent { at: now });
    }

    fn evict_old(&mut self, now: Instant) {
        let cutoff = now - RATE_WINDOW;
        while self.recent.front().map(|e| e.at < cutoff).unwrap_or(false) {
            self.recent.pop_front();
        }
    }

    fn avg_duration_ms(&self) -> u64 {
        if self.total_calls == 0 { return 0; }
        self.duration_sum_ms / self.total_calls
    }
}

pub struct Analyzer {
    processes: Vec<ProcessInfo>,
    port_to_pid: HashMap<u16, u32>,
    edges: HashMap<(u32, u32), EdgeState>,
    /// Inbound request tracking keyed by listening port
    port_states: HashMap<u16, PortState>,
    /// Established connections from previous tick (for new-connection detection)
    prev_conns: HashSet<ConnKey>,
    /// HTTP requests from the tap proxy
    http_log: VecDeque<HttpEntry>,
    /// Per-path aggregates from the tap
    path_states: HashMap<String, PathState>,
    /// Listen port of the tap proxy (if active)
    tap_listen_port: Option<u16>,
}

const HTTP_LOG_MAX: usize = 200;

impl Analyzer {
    pub fn new() -> Self {
        Self {
            processes: vec![],
            port_to_pid: HashMap::new(),
            edges: HashMap::new(),
            port_states: HashMap::new(),
            prev_conns: HashSet::new(),
            http_log: VecDeque::new(),
            path_states: HashMap::new(),
            tap_listen_port: None,
        }
    }

    pub fn set_tap_port(&mut self, port: u16) {
        self.tap_listen_port = Some(port);
    }

    pub async fn run(
        &mut self,
        mut rx: mpsc::Receiver<CollectorEvent>,
        tx: mpsc::Sender<GraphSnapshot>,
    ) {
        while let Some(event) = rx.recv().await {
            match event {
                CollectorEvent::ProcessSnapshot(mut procs) => {
                    node_bun::enrich(&mut procs);
                    self.processes = procs;
                }
                CollectorEvent::ConnectionSnapshot(conns) => {
                    self.integrate_connections(conns);
                    let snapshot = self.build_snapshot();
                    let _ = tx.try_send(snapshot);
                }
                CollectorEvent::HttpRequest { method, path, status, duration_ms, timestamp } => {
                    self.record_http(method, path, status, duration_ms, timestamp);
                    // Send a snapshot immediately so the UI updates on each request
                    let snapshot = self.build_snapshot();
                    let _ = tx.try_send(snapshot);
                }
            }
        }
    }

    fn integrate_connections(&mut self, conns: Vec<SocketConn>) {
        let now = Instant::now();

        // ── Step 1: build port→pid from p.ports (populated by collector) ─────
        // On macOS, p.ports is filled by lsof in the process scanner.
        // On Linux, p.ports starts empty and is refined by inode matching below.
        self.port_to_pid.clear();
        for p in &self.processes {
            for &port in &p.ports {
                self.port_to_pid.insert(port, p.pid);
            }
        }

        // ── Step 2: refine with socket inode mapping (Linux only) ────────────
        // This also dynamically discovers ports for processes not yet in p.ports.
        let inode_to_pid: HashMap<u64, u32> = self
            .processes
            .iter()
            .flat_map(|p| p.socket_inodes.iter().map(move |&inode| (inode, p.pid)))
            .collect();

        for conn in conns.iter().filter(|c| c.is_listen()) {
            if let Some(&pid) = inode_to_pid.get(&conn.inode) {
                if let Some(p) = self.processes.iter_mut().find(|p| p.pid == pid) {
                    if !p.ports.contains(&conn.local_port) {
                        p.ports.push(conn.local_port);
                    }
                }
                // Inode match is authoritative — overwrite any stale entry
                self.port_to_pid.insert(conn.local_port, pid);
            }
            // macOS: inode == 0, but port_to_pid already correct from Step 1
        }

        // ── Step 2: count active connections per known port ───────────────────
        // This gives us "how many clients are currently connected right now"
        let mut active_per_port: HashMap<u16, u64> = HashMap::new();
        for conn in conns.iter().filter(|c| c.is_established()) {
            if self.port_to_pid.contains_key(&conn.local_port) {
                *active_per_port.entry(conn.local_port).or_insert(0) += 1;
            }
        }

        // ── Step 3: detect NEW established connections ────────────────────────
        // A connection key that exists now but didn't last tick = a freshly accepted
        // TCP connection = one HTTP request on that port.
        let current_conns: HashSet<ConnKey> = conns
            .iter()
            .filter(|c| c.is_established())
            .map(|c| ConnKey { local_port: c.local_port, remote_port: c.remote_port })
            .collect();

        let new_conns: Vec<&SocketConn> = conns
            .iter()
            .filter(|c| {
                c.is_established()
                    && !self.prev_conns.contains(&ConnKey {
                        local_port: c.local_port,
                        remote_port: c.remote_port,
                    })
            })
            .collect();

        for conn in &new_conns {
            // ── Service-to-service edge tracking ─────────────────────────────
            let src_pid = inode_to_pid.get(&conn.inode).copied()
                .or_else(|| self.port_to_pid.get(&conn.local_port).copied());
            let dst_pid = self.port_to_pid.get(&conn.remote_port).copied();

            if let (Some(src), Some(dst)) = (src_pid, dst_pid) {
                if src != dst && src != 0 && dst != 0 {
                    self.edges.entry((src, dst))
                        .or_insert_with(|| EdgeState::new(now))
                        .record_call(now);
                }
            }

            // ── Inbound request tracking: new connection TO a listening port ──
            // local_port == server port means the server just accepted this connection.
            if let Some(&pid) = self.port_to_pid.get(&conn.local_port) {
                if pid != 0 {
                    self.port_states
                        .entry(conn.local_port)
                        .or_insert_with(|| PortState::new(pid, now))
                        .record_request(now);
                }
            }
        }

        self.prev_conns = current_conns;

        // ── Step 4: update active connection counts and evict stale data ──────
        for (port, state) in self.port_states.iter_mut() {
            state.active_connections = *active_per_port.get(port).unwrap_or(&0);
            state.evict_old_events(now);
        }

        for state in self.edges.values_mut() {
            state.evict_old_events(now);
        }
        self.edges.retain(|_, v| now - v.last_seen < EDGE_TTL);
    }

    fn build_snapshot(&self) -> GraphSnapshot {
        let service_name_map: HashMap<u32, String> = self
            .processes
            .iter()
            .map(|p| (p.pid, p.service_name.clone()))
            .collect();

        let services: Vec<Service> = self
            .processes
            .iter()
            .map(|p| Service {
                pid: p.pid,
                name: p.service_name.clone(),
                runtime: p.runtime.clone(),
                script: p.script.clone(),
                cwd: p.cwd.as_ref().map(|c| c.to_string_lossy().to_string()),
                ports: p.ports.clone(),
            })
            .collect();

        let now = Instant::now();

        let edges: Vec<Edge> = self
            .edges
            .iter()
            .map(|((src_pid, dst_pid), state)| Edge {
                src_pid: *src_pid,
                dst_pid: *dst_pid,
                total_calls: state.total_calls,
                recent_calls: state.recent_calls(),
                window_secs: RATE_WINDOW.as_secs(),
                idle_secs: (now - state.last_seen).as_secs(),
            })
            .collect();

        let port_stats: Vec<PortStat> = self
            .port_states
            .iter()
            .map(|(&port, state)| PortStat {
                port,
                pid: state.pid,
                service_name: service_name_map
                    .get(&state.pid)
                    .cloned()
                    .unwrap_or_else(|| format!("pid:{}", state.pid)),
                total_requests: state.total_requests,
                recent_requests: state.recent_requests(),
                window_secs: RATE_WINDOW.as_secs(),
                active_connections: state.active_connections,
                idle_secs: (now - state.last_seen).as_secs(),
            })
            .collect();

        let now_chrono = chrono::Utc::now();
        let http_log: Vec<HttpEntry> = self.http_log.iter().cloned().collect();

        let path_stats: Vec<PathStat> = self
            .path_states
            .iter()
            .map(|(path, state)| PathStat {
                path: path.clone(),
                total_calls: state.total_calls,
                recent_calls: state.recent.len() as u64,
                window_secs: RATE_WINDOW.as_secs(),
                avg_duration_ms: state.avg_duration_ms(),
                last_status: state.last_status,
                error_count: state.error_count,
            })
            .collect();

        GraphSnapshot {
            services,
            edges,
            port_stats,
            http_log,
            path_stats,
            tap_listen_port: self.tap_listen_port,
            timestamp: now_chrono,
        }
    }

    fn record_http(
        &mut self,
        method: String,
        path: String,
        status: u16,
        duration_ms: u64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) {
        let now = Instant::now();

        // Normalise path: strip query string for aggregation key
        let path_key = path.split('?').next().unwrap_or(&path).to_string();

        self.path_states
            .entry(path_key)
            .or_insert_with(PathState::new)
            .record(now, status, duration_ms);

        // Evict old rate events across all paths
        for state in self.path_states.values_mut() {
            state.evict_old(now);
        }

        self.http_log.push_back(HttpEntry {
            method,
            path,
            status,
            duration_ms,
            timestamp,
        });

        // Keep the log bounded
        while self.http_log.len() > HTTP_LOG_MAX {
            self.http_log.pop_front();
        }
    }
}
