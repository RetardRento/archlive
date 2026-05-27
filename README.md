# arch-live

Zero-config real-time architecture visualizer for Node.js and Bun applications.

[![GitHub release](https://img.shields.io/github/v/release/RetardRento/archlive?style=flat-square)](https://github.com/RetardRento/archlive/releases/latest)
<!-- archlive-version:v0.1.0 -->

## Install

### Homebrew (macOS / Linux)

```bash
brew install RetardRento/tap/arch-live
```

### Pre-built binaries

**[⬇ Latest release](https://github.com/RetardRento/archlive/releases/latest)**

| Platform | Asset |
|----------|--------|
| macOS (Apple Silicon) | `arch-live-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `arch-live-<version>-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `arch-live-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (ARM64) | `arch-live-<version>-aarch64-unknown-linux-gnu.tar.gz` |

#### macOS (Apple Silicon)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-apple-darwin/arch-live" /usr/local/bin/arch-live
```

#### macOS (Intel)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-apple-darwin/arch-live" /usr/local/bin/arch-live
```

#### Linux (x86_64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

#### Linux (ARM64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

Checksums are attached to each release (e.g. `arch-live-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256`).

### Build from source

```bash
# requires Rust: https://rustup.rs
git clone https://github.com/RetardRento/archlive.git
cd archlive
cargo build --release
# binary at: ./target/release/arch-live
```

---

## Usage

```bash
arch-live                        # scan everything, refresh every 1s
arch-live --node-only            # focus on Node.js and Bun processes
arch-live --refresh-rate 0.5     # faster refresh (500ms)
arch-live --port-filter 3000     # only show connections on port 3000
arch-live --tap 3001:3000        # HTTP tap proxy (see below)
```

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--refresh-rate <SECONDS>` | `1.0` | Polling interval (decimals OK, min 0.1s) |
| `--port-filter <PORT>` | — | Show only connections involving this port |
| `--node-only` | off | Focus on Node.js and Bun processes only |
| `--tap <LISTEN:TARGET>` | — | HTTP tap proxy — see [HTTP tap proxy](#http-tap-proxy---tap) |

### Keyboard controls

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `n` | Toggle Node/Bun-only view |
| `↑` / `↓` | Navigate service list |
| `r` | Force refresh |

---

## HTTP tap proxy (`--tap`)

The tap proxy sits between your frontend and backend, intercepting every HTTP request to capture method, path, status code, and duration — without modifying your application code.

```bash
arch-live --tap 3001:3000
```

```
Frontend  →  :3001 (arch-live tap)  →  :3000 (your backend)
                      ↕
              logs every request
```

Point your frontend at the tap port instead of the backend directly:

```bash
# backend runs on :3000
node server.js

# start arch-live with tap
arch-live --tap 3001:3000

# frontend calls :3001 — arch-live intercepts and forwards to :3000
curl http://localhost:3001/api/users
```

Every intercepted request appears in the Live Events panel:

```
[14:32:01] GET  /api/users        200  12ms
[14:32:02] POST /api/orders       201  34ms
[14:32:03] GET  /api/missing      404   8ms
```

> **Note:** Works for plain HTTP. For HTTPS, terminate TLS before the tap (e.g. a local `mkcert` proxy).

---

## Example — detecting a Node.js + Bun app

Start two services in separate terminals:

```bash
# Terminal 1: an Express API
node server.js          # listening on :3000

# Terminal 2: a Bun frontend that calls the API
bun run index.ts        # listening on :8080, fetches from :3000
```

Then run arch-live:

```bash
arch-live --node-only
```

You will see:

```
┌─ Services ──────────────────┐  ┌─ Architecture Graph ─────────────────────────┐
│ [node]  server (Express)  :3000 │  │ server               ──→ (external)                │
│ [bun]   index (Bun.serve) :8080 │  │ index                ──→ server              (4 calls) │
└─────────────────────────────┘  └──────────────────────────────────────────────┘
┌─ Live Events ────────────────────────────────────────────────────────────────────┐
│ [14:32:01] New connection: index → server                                        │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## Platform support

| OS | Process scan | Socket scan | Pre-built binary |
|----|-------------|-------------|-----------------|
| Linux | `/proc/<pid>/` | `/proc/net/tcp[6]` | ✅ x86_64, ARM64 |
| macOS | `ps -eo pid,comm,args` | `netstat -an` | ✅ Intel, Apple Silicon |
| Other | — | — | Build from source |

---

## Architecture

```
main.rs
  ├── collector/          System data ingestion (non-blocking)
  │     ├── process.rs    /proc (Linux) or ps (macOS) scanning
  │     ├── socket.rs     /proc/net/tcp or netstat scanning
  │     └── node_bun.rs   Node.js/Bun heuristics
  ├── tap/                HTTP reverse proxy interceptor
  │     └── mod.rs        Transparent proxy: captures method/path/status/duration
  ├── analyzer/           Graph building
  │     ├── mod.rs        Joins process + socket data, maintains edge state
  │     └── graph.rs      Service / Edge / GraphSnapshot types
  └── renderer/           TUI
        ├── mod.rs        Terminal setup, event loop
        ├── ui.rs         ratatui layout (title / services / graph / log)
        └── events.rs     WebSocket / JSONL export stub
```

Data flows through tokio mpsc channels:

```
Collector ──(CollectorEvent)──▶ Analyzer ──(GraphSnapshot)──▶ Renderer
```

---

## Roadmap

- **Node.js require-hook** — patch `http`/`https` for route-level tracing
- **Bun fetch-hook** — intercept `fetch()` and `Bun.serve()` via preload script
- **eBPF collector** — low-level kernel tracing, no process injection needed
- **Web dashboard** — export graph over WebSocket as live JSON
