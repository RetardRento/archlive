pub mod process;
pub mod socket;
pub mod node_bun;

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tracing::{debug, warn};

pub use process::{ProcessInfo, Runtime};
pub use socket::SocketConn;

/// Events emitted by the collector to the analyzer
#[derive(Debug, Clone)]
pub enum CollectorEvent {
    /// A snapshot of all discovered services this tick
    ProcessSnapshot(Vec<ProcessInfo>),
    /// A snapshot of all active TCP connections this tick
    ConnectionSnapshot(Vec<SocketConn>),
    /// A single HTTP request intercepted by the tap proxy
    HttpRequest {
        method: String,
        path: String,
        status: u16,
        duration_ms: u64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

pub struct Collector {
    refresh_ms: u64,
    port_filter: Option<u16>,
    node_only: bool,
}

impl Collector {
    pub fn new(refresh_ms: u64, port_filter: Option<u16>, node_only: bool) -> Self {
        Self { refresh_ms, port_filter, node_only }
    }

    pub async fn run(&mut self, tx: mpsc::Sender<CollectorEvent>) {
        let mut interval = time::interval(Duration::from_millis(self.refresh_ms));

        loop {
            interval.tick().await;

            // Scan processes
            match process::scan_processes(self.node_only) {
                Ok(procs) => {
                    debug!("Collected {} processes", procs.len());
                    if tx.send(CollectorEvent::ProcessSnapshot(procs)).await.is_err() {
                        break;
                    }
                }
                Err(e) => warn!("Process scan error: {}", e),
            }

            // Scan TCP connections
            match socket::scan_connections(self.port_filter) {
                Ok(conns) => {
                    debug!("Collected {} connections", conns.len());
                    if tx.send(CollectorEvent::ConnectionSnapshot(conns)).await.is_err() {
                        break;
                    }
                }
                Err(e) => warn!("Socket scan error: {}", e),
            }
        }
    }
}
