pub mod ui;
pub mod events;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use crate::analyzer::GraphSnapshot;
use crate::cli::Args;

pub struct Renderer {
    args: Args,
    /// Currently selected service index in the left panel
    selected_idx: usize,
    /// Whether node-only view is active (toggled with 'n')
    node_only: bool,
    /// Recent log entries for the bottom events panel
    event_log: Vec<String>,
    /// Last received graph snapshot
    last_snapshot: Option<GraphSnapshot>,
}

impl Renderer {
    pub fn new(args: Args) -> Result<Self> {
        Ok(Self {
            node_only: args.node_only,
            args,
            selected_idx: 0,
            event_log: Vec::new(),
            last_snapshot: None,
        })
    }

    pub async fn run(&mut self, mut graph_rx: mpsc::Receiver<GraphSnapshot>) -> Result<()> {
        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick_rate = Duration::from_millis(self.args.refresh_rate_ms());
        let mut last_tick = Instant::now();

        let result = self.event_loop(&mut terminal, &mut graph_rx, tick_rate, &mut last_tick).await;

        // Restore terminal regardless of error
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        graph_rx: &mut mpsc::Receiver<GraphSnapshot>,
        tick_rate: Duration,
        last_tick: &mut Instant,
    ) -> Result<()> {
        loop {
            // Drain any pending graph snapshots (non-blocking)
            while let Ok(snapshot) = graph_rx.try_recv() {
                self.on_new_snapshot(&snapshot);
                self.last_snapshot = Some(snapshot);
            }

            // Draw frame
            terminal.draw(|f| ui::draw(f, self))?;

            // Poll for keyboard events with a short timeout so we stay responsive
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_millis(0));

            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('r') => {
                                // Force-clear snapshot so next tick re-draws clean
                                self.last_snapshot = None;
                            }
                            KeyCode::Char('n') => {
                                self.node_only = !self.node_only;
                                self.log_event(format!(
                                    "Node/Bun-only view: {}",
                                    if self.node_only { "ON" } else { "OFF" }
                                ));
                            }
                            KeyCode::Down => {
                                let max = self
                                    .last_snapshot
                                    .as_ref()
                                    .map(|s| s.services.len().saturating_sub(1))
                                    .unwrap_or(0);
                                self.selected_idx = (self.selected_idx + 1).min(max);
                            }
                            KeyCode::Up => {
                                self.selected_idx = self.selected_idx.saturating_sub(1);
                            }
                            _ => {}
                        }
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                *last_tick = Instant::now();
            }
        }
    }

    fn on_new_snapshot(&mut self, snapshot: &GraphSnapshot) {
        let service_map: std::collections::HashMap<u32, String> = snapshot
            .services
            .iter()
            .map(|s| (s.pid, s.name.clone()))
            .collect();

        // ── Log new HTTP tap requests ─────────────────────────────────────────
        if let Some(prev) = &self.last_snapshot {
            let prev_len = prev.http_log.len();
            for entry in snapshot.http_log.iter().skip(prev_len) {
                let status_marker = match entry.status {
                    200..=299 => "✓",
                    300..=399 => "→",
                    400..=499 => "!",
                    500..=599 => "✗",
                    _ => "?",
                };
                self.log_event(format!(
                    "{} {} {}  {} ({})",
                    entry.method,
                    entry.path,
                    entry.status,
                    status_marker,
                    entry.duration_label(),
                ));
            }
        }

        // ── Log new inbound requests per port ─────────────────────────────────
        // Compare total_requests to what we had last tick; delta = new requests
        let prev_port_totals: std::collections::HashMap<u16, u64> = self
            .last_snapshot
            .as_ref()
            .map(|prev| prev.port_stats.iter().map(|p| (p.port, p.total_requests)).collect())
            .unwrap_or_default();

        for ps in &snapshot.port_stats {
            let prev = prev_port_totals.get(&ps.port).copied().unwrap_or(0);
            let new_reqs = ps.total_requests.saturating_sub(prev);
            if new_reqs == 0 {
                continue;
            }
            self.log_event(format!(
                "→ :{} ({})  +{} request{}  total: {}  rate: {}  active: {}",
                ps.port,
                ps.service_name,
                new_reqs,
                if new_reqs == 1 { "" } else { "s" },
                ps.total_requests,
                ps.rate_label(),
                ps.active_connections,
            ));
        }

        // ── Log new service-to-service calls ──────────────────────────────────
        let prev_edge_counts: std::collections::HashMap<(u32, u32), u64> = self
            .last_snapshot
            .as_ref()
            .map(|prev| {
                prev.edges
                    .iter()
                    .map(|e| ((e.src_pid, e.dst_pid), e.total_calls))
                    .collect()
            })
            .unwrap_or_default();

        for edge in &snapshot.edges {
            let prev = prev_edge_counts.get(&(edge.src_pid, edge.dst_pid)).copied().unwrap_or(0);
            let new_calls = edge.total_calls.saturating_sub(prev);
            if new_calls == 0 {
                continue;
            }
            let src = service_map.get(&edge.src_pid).cloned()
                .unwrap_or_else(|| format!("pid:{}", edge.src_pid));
            let dst = service_map.get(&edge.dst_pid).cloned()
                .unwrap_or_else(|| format!("pid:{}", edge.dst_pid));
            self.log_event(format!(
                "  {} ↔ {}  +{}  (total: {}  rate: {})",
                src, dst, new_calls, edge.total_calls, edge.rate_label(),
            ));
        }
    }

    pub fn log_event(&mut self, msg: String) {
        let ts = chrono::Local::now().format("%H:%M:%S");
        self.event_log.push(format!("[{}] {}", ts, msg));
        // Keep the log bounded
        if self.event_log.len() > 200 {
            self.event_log.remove(0);
        }
    }
}
