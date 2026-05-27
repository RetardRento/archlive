# arch-live

Zero-config real-time architecture visualizer for Node.js and Bun applications.

**Repository:** [github.com/RetardRento/archlive](https://github.com/RetardRento/archlive)

## Install (pre-built binaries)

Download the latest release for your platform from GitHub:

**[⬇ Latest release](https://github.com/RetardRento/archlive/releases/latest)**

| Platform | Asset |
|----------|--------|
| macOS (Apple Silicon) | `arch-live-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `arch-live-<version>-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `arch-live-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (ARM64) | `arch-live-<version>-aarch64-unknown-linux-gnu.tar.gz` |

Replace `<version>` with the release tag (without `v`), e.g. `0.1.0`.

### macOS (Apple Silicon)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-apple-darwin/arch-live" /usr/local/bin/arch-live
arch-live --help
```

### macOS (Intel)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-apple-darwin/arch-live" /usr/local/bin/arch-live
```

### Linux (x86_64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

### Linux (ARM64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/RetardRento/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

> **Tip:** For the newest build without editing `VERSION`, open the [latest release](https://github.com/RetardRento/archlive/releases/latest) page and copy the download URL for your platform.

Each release also includes `.sha256` checksum files you can verify with `shasum -a 256 -c arch-live-*.tar.gz.sha256` (after renaming the checksum file to match the archive name, or by comparing manually).

## Build from source

### Prerequisites

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build

```bash
git clone https://github.com/RetardRento/archlive.git
cd archlive
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

| OS | Process scan | Socket scan | Pre-built release |
|----|-------------|-------------|-------------------|
| Linux | `/proc/<pid>/` | `/proc/net/tcp[6]` | ✅ x86_64, ARM64 |
| macOS | `ps -eo pid,comm,args` | `netstat -an` | ✅ Intel, Apple Silicon |
| Other | — | — | Build from source |

## Releasing a new version (maintainers)

Releases are automated with [GitHub Actions](https://github.com/RetardRento/archlive/actions).

1. Bump the version in `Cargo.toml` (must match the git tag, without the `v` prefix).
2. Commit and push to `main`.
3. Create and push a tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The **Release** workflow builds binaries for all supported targets, uploads them to a [GitHub Release](https://github.com/RetardRento/archlive/releases), and generates release notes.

To re-run a release manually: **Actions → Release → Run workflow** and enter the tag (e.g. `v0.1.0`).

The **CI** workflow runs on every push/PR to `main` and verifies `cargo build` and `cargo test` on Linux and macOS.

## Future roadmap (stubs in code)

- **Node.js require-hook** — patch `http`/`https` for route-level tracing (`collector/node_bun.rs:inject_node_require_hook`)
- **Bun fetch-hook** — intercept `fetch()` and `Bun.serve()` via preload script
- **eBPF collector** — low-level kernel tracing, no process injection needed
- **Web dashboard** — export graph over WebSocket as live JSON
