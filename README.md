# arch-live

Zero-config real-time architecture visualizer for Node.js and Bun applications.

**Repository:** [github.com/karthikeyasomayajula/archlive](https://github.com/karthikeyasomayajula/archlive)

[![GitHub release](https://img.shields.io/github/v/release/karthikeyasomayajula/archlive?style=flat-square)](https://github.com/karthikeyasomayajula/archlive/releases/latest)
<!-- archlive-version:v0.1.0 -->

## Install

Each [release](https://github.com/karthikeyasomayajula/archlive/releases) is tagged (e.g. `v0.1.0`) with versioned binaries. The examples below pin a specific version — after a new release, CI updates these snippets on `main` automatically.

**[⬇ Latest release](https://github.com/karthikeyasomayajula/archlive/releases/latest)**

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
  "https://github.com/karthikeyasomayajula/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-apple-darwin/arch-live" /usr/local/bin/arch-live
```

#### macOS (Intel)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/karthikeyasomayajula/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-apple-darwin.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-apple-darwin/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-apple-darwin/arch-live" /usr/local/bin/arch-live
```

#### Linux (x86_64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/karthikeyasomayajula/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-x86_64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

#### Linux (ARM64)

```bash
VERSION=0.1.0
curl -fsSL -o arch-live.tar.gz \
  "https://github.com/karthikeyasomayajula/archlive/releases/download/v${VERSION}/arch-live-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
tar -xzf arch-live.tar.gz
chmod +x "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live"
sudo mv "arch-live-${VERSION}-aarch64-unknown-linux-gnu/arch-live" /usr/local/bin/arch-live
```

Checksums are attached to each release (e.g. `arch-live-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256`).

### Build from source

```bash
# requires Rust: https://rustup.rs
git clone https://github.com/karthikeyasomayajula/archlive.git
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

## Releasing a new version (maintainers)

Releases are **fully automated** via [release-plz](https://release-plz.ieni.dev/) and [GitHub Actions](https://github.com/karthikeyasomayajula/archlive/actions). No manual version bumps or tagging needed.

### Automated flow

1. Merge a PR that changes `src/` or `Cargo.toml` into `main`
2. **Release PLZ** workflow opens a "Release PR" with:
   - Bumped version in `Cargo.toml` (based on [conventional commits](#commit-convention))
   - Updated `CHANGELOG.md`
3. Review and merge the Release PR
4. **Release PLZ** creates the git tag automatically
5. **Release** workflow triggers and:
   - Builds cross-compiled binaries for all platforms
   - Publishes a tagged [GitHub Release](https://github.com/karthikeyasomayajula/archlive/releases) with generated release notes
   - Updates README install snippets on `main`
   - Updates the Homebrew tap formula (if `HOMEBREW_TAP_TOKEN` secret is configured)

### Commit convention

Version bumps follow [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix | Bump |
|--------|------|
| `fix:` | patch (`0.1.0` → `0.1.1`) |
| `feat:` | minor (`0.1.0` → `0.2.0`) |
| `feat!:` / `BREAKING CHANGE:` | major (`0.1.0` → `1.0.0`) |
| `chore:` / `docs:` / `ci:` | no release triggered |

### Homebrew tap setup (one-time)

1. Create repo `karthikeyasomayajula/homebrew-tap` with a `Formula/` directory
2. Create a GitHub PAT with `repo` write access to that repo
3. Add it as a repository secret named `HOMEBREW_TAP_TOKEN` in this repo
4. Users can then install via:
   ```bash
   brew tap karthikeyasomayajula/tap
   brew install arch-live
   ```

### Manual re-release

To re-run a release for an existing tag: **Actions → Release → Run workflow** and enter the tag (e.g. `v0.1.0`).

The **CI** workflow runs on every push/PR to `main` and verifies `cargo build` and `cargo test` on Linux and macOS.

## Future roadmap (stubs in code)

- **Node.js require-hook** — patch `http`/`https` for route-level tracing (`collector/node_bun.rs:inject_node_require_hook`)
- **Bun fetch-hook** — intercept `fetch()` and `Bun.serve()` via preload script
- **eBPF collector** — low-level kernel tracing, no process injection needed
- **Web dashboard** — export graph over WebSocket as live JSON
