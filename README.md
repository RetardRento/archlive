# arch-live

Zero-config real-time architecture visualizer for Node.js and Bun applications.

## Build & Run

### Prerequisites

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build

```bash
cargo build --release
# Binary at: ./target/release/arch-live
```

### Run

```bash
# Basic — scan everything, refresh every 1 second
./target/release/arch-live

# Focus on Node.js / Bun processes only
./target/release/arch-live --node-only

# Faster refresh (500ms)
./target/release/arch-live --refresh-rate 0.5

# Filter to a specific port
./target/release/arch-live --port-filter 3000

# During development (cargo run)
cargo run -- --node-only
```

### Keyboard controls

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `n` | Toggle Node/Bun-only view |
| `↑` / `↓` | Navigate service list |
| `r` | Force refresh |

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

## Architecture

```
main.rs
  ├── collector/          System data ingestion (non-blocking)
  │     ├── process.rs    /proc (Linux) or ps (macOS) scanning
  │     ├── socket.rs     /proc/net/tcp or netstat scanning
  │     └── node_bun.rs   Node.js/Bun heuristics + future hook stubs
  ├── analyzer/           Graph building
  │     ├── mod.rs        Joins process + socket data, maintains edge state
  │     └── graph.rs      Service / Edge / GraphSnapshot types
  └── renderer/           TUI
        ├── mod.rs        Terminal setup, event loop
        ├── ui.rs         ratatui layout (title / services / graph / log)
        └── events.rs     Stub for future WebSocket / JSONL export
```

Data flows through tokio mpsc channels:

```
Collector ──(CollectorEvent)──▶ Analyzer ──(GraphSnapshot)──▶ Renderer
```

## Platform support

| OS | Process scan | Socket scan |
|----|-------------|-------------|
| Linux | `/proc/<pid>/` | `/proc/net/tcp[6]` |
| macOS | `ps -eo pid,comm,args` | `netstat -an` |

## Future roadmap (stubs in code)

- **Node.js require-hook** — patch `http`/`https` for route-level tracing (`collector/node_bun.rs:inject_node_require_hook`)
- **Bun fetch-hook** — intercept `fetch()` and `Bun.serve()` via preload script
- **eBPF collector** — low-level kernel tracing, no process injection needed
- **Web dashboard** — export graph over WebSocket as live JSON
