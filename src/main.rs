mod cli;
mod collector;
mod analyzer;
mod renderer;
mod tap;

use anyhow::Result;
use clap::Parser;
use cli::Args;
use tokio::sync::mpsc;
use tracing_subscriber::{fmt, EnvFilter};

use collector::CollectorEvent;
use analyzer::Analyzer;
use renderer::Renderer;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_ok() {
        fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }

    let args = Args::parse();

    // collector → analyzer
    let (collector_tx, collector_rx) = mpsc::channel::<CollectorEvent>(512);
    // analyzer → renderer
    let (graph_tx, graph_rx) = mpsc::channel::<analyzer::GraphSnapshot>(64);

    let refresh_rate = args.refresh_rate_ms();
    let port_filter = args.port_filter;
    let node_only = args.node_only;
    let tap_cfg = args.tap.clone();

    // Spawn collector
    let collector_tx2 = collector_tx.clone();
    tokio::spawn(async move {
        let mut c = collector::Collector::new(refresh_rate, port_filter, node_only);
        c.run(collector_tx2).await;
    });

    // Spawn HTTP tap proxy (if --tap was given)
    if let Some(ref cfg) = tap_cfg {
        let tap_tx = collector_tx.clone();
        let listen = cfg.listen_port;
        let target = cfg.target_port;
        tokio::spawn(async move {
            tap::run(listen, target, tap_tx).await;
        });
    }

    // Spawn analyzer (knows about the tap port for display purposes)
    let tap_listen_port = tap_cfg.as_ref().map(|c| c.listen_port);
    tokio::spawn(async move {
        let mut a = Analyzer::new();
        if let Some(port) = tap_listen_port {
            a.set_tap_port(port);
        }
        a.run(collector_rx, graph_tx).await;
    });

    // Renderer runs on the main thread
    let mut renderer = Renderer::new(args)?;
    renderer.run(graph_rx).await?;

    Ok(())
}
