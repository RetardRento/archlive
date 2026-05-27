use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "arch-live",
    about = "Zero-config real-time architecture visualizer for Node.js and Bun apps",
    version
)]
pub struct Args {
    /// Refresh rate in seconds (decimals allowed, e.g. 0.5)
    #[arg(long, default_value = "1.0", value_name = "SECONDS")]
    pub refresh_rate: f64,

    /// Only show connections involving this port
    #[arg(long, value_name = "PORT")]
    pub port_filter: Option<u16>,

    /// Focus only on Node.js and Bun processes
    #[arg(long)]
    pub node_only: bool,

    /// Enable HTTP tap proxy. Format: LISTEN_PORT:TARGET_PORT
    /// arch-live listens on LISTEN_PORT and forwards to TARGET_PORT,
    /// capturing every request (method, path, status, duration).
    ///
    /// Example: --tap 3001:3000
    ///   Your backend runs on :3000
    ///   Point your frontend at :3001
    ///   arch-live intercepts and logs every HTTP call
    ///
    /// Note: works for plain HTTP. For HTTPS, terminate TLS at a proxy first.
    #[arg(long, value_name = "LISTEN:TARGET", value_parser = parse_tap)]
    pub tap: Option<TapConfig>,
}

#[derive(Debug, Clone)]
pub struct TapConfig {
    pub listen_port: u16,
    pub target_port: u16,
}

fn parse_tap(s: &str) -> Result<TapConfig, String> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err("expected format LISTEN_PORT:TARGET_PORT, e.g. 3001:3000".to_string());
    }
    let listen_port = parts[0]
        .parse::<u16>()
        .map_err(|_| format!("invalid listen port: {}", parts[0]))?;
    let target_port = parts[1]
        .parse::<u16>()
        .map_err(|_| format!("invalid target port: {}", parts[1]))?;
    Ok(TapConfig { listen_port, target_port })
}

impl Args {
    pub fn refresh_rate_ms(&self) -> u64 {
        (self.refresh_rate * 1000.0).max(100.0) as u64
    }
}
