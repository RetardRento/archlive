/// Node.js / Bun specific heuristics and future hook scaffolding.
///
/// This module centralises all runtime-specific logic so it's easy to
/// extend with deeper instrumentation later (e.g. require-hook patching
/// for Node.js or fetch-interception for Bun).

use crate::collector::process::{ProcessInfo, Runtime};

/// Common framework patterns detected by heuristic analysis of the process
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameworkHint {
    Express,
    Fastify,
    Hono,
    BunServe,
    NextJs,
    Unknown,
}

impl FrameworkHint {
    pub fn label(&self) -> &'static str {
        match self {
            FrameworkHint::Express => "Express",
            FrameworkHint::Fastify => "Fastify",
            FrameworkHint::Hono => "Hono",
            FrameworkHint::BunServe => "Bun.serve",
            FrameworkHint::NextJs => "Next.js",
            FrameworkHint::Unknown => "",
        }
    }
}

/// Enrich a process list with Node/Bun specific metadata.
/// Right now this is pure heuristics; future versions can inject
/// runtime hooks to get authoritative data.
pub fn enrich(processes: &mut Vec<ProcessInfo>) {
    for p in processes.iter_mut() {
        match p.runtime {
            Runtime::Node | Runtime::Bun => {
                let hint = detect_framework(p);
                // Annotate the service name with the framework if detected
                if hint != FrameworkHint::Unknown {
                    p.service_name = format!("{} ({})", p.service_name, hint.label());
                }
            }
            _ => {}
        }
    }
}

fn detect_framework(p: &ProcessInfo) -> FrameworkHint {
    let script = p.script.as_deref().unwrap_or("").to_lowercase();
    let cwd = p
        .cwd
        .as_ref()
        .map(|c| c.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Next.js: typically runs as `node node_modules/.bin/next` or cwd contains next.config
    if script.contains("next") || cwd.contains("nextjs") || cwd.contains("next-") {
        return FrameworkHint::NextJs;
    }
    // Bun.serve() — Bun runtime with a serve-style script
    if matches!(p.runtime, Runtime::Bun) && (script.contains("serve") || script.contains("server"))
    {
        return FrameworkHint::BunServe;
    }
    // Heuristic: script name contains "fastify"
    if script.contains("fastify") {
        return FrameworkHint::Fastify;
    }
    // Heuristic: script name contains "hono"
    if script.contains("hono") {
        return FrameworkHint::Hono;
    }
    // Express is the most common; hard to detect without reading package.json.
    // We guess based on common entry-point names.
    if script.contains("server") || script.contains("app") || script.contains("index") {
        if matches!(p.runtime, Runtime::Node) {
            return FrameworkHint::Express;
        }
    }

    FrameworkHint::Unknown
}

// ── Future hook scaffolding ───────────────────────────────────────────────────
//
// The functions below are stubs for runtime instrumentation that will be
// implemented in a future release. They are intentionally left as documented
// dead code so the design is preserved.

/// (Future) Inject a Node.js require-hook by writing a loader script and
/// setting NODE_OPTIONS=--require /path/to/arch-live-hook.js on the process.
#[allow(dead_code)]
pub async fn inject_node_require_hook(_pid: u32) -> anyhow::Result<()> {
    // Implementation plan:
    // 1. Write a small CJS loader to a temp file that patches http/https .request()
    //    and emits JSON events to a Unix socket.
    // 2. Use /proc/<pid>/environ + SIGSTOP + ptrace to inject NODE_OPTIONS,
    //    or communicate via a side-channel if the process was started by arch-live.
    // 3. Receive events on the Unix socket and forward to the analyzer channel.
    todo!("Node.js require-hook injection")
}

/// (Future) Attach to a Bun process and intercept fetch() calls via
/// Bun's --preload mechanism or IPC channel.
#[allow(dead_code)]
pub async fn inject_bun_fetch_hook(_pid: u32) -> anyhow::Result<()> {
    // Implementation plan:
    // 1. Generate a preload script that wraps globalThis.fetch and Bun.serve.
    // 2. Use Bun's IPC API to stream call events back to arch-live.
    // 3. Parse events in the collector and emit CollectorEvents.
    todo!("Bun fetch-hook injection")
}
