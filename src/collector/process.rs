use anyhow::{Context, Result};
#[cfg(target_os = "linux")]
use std::fs;
use std::path::PathBuf;

/// Runtime classification of the process
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Runtime {
    Node,
    Bun,
    Other(String),
}

impl Runtime {
    pub fn label(&self) -> &str {
        match self {
            Runtime::Node => "node",
            Runtime::Bun => "bun",
            Runtime::Other(s) => s.as_str(),
        }
    }
}

/// All metadata we've extracted about a process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub runtime: Runtime,
    /// The primary script/entry-point file (e.g. server.js)
    pub script: Option<String>,
    /// Working directory of the process
    pub cwd: Option<PathBuf>,
    /// Derived display name: script stem or folder name
    pub service_name: String,
    /// Ports this process is listening on (filled by socket scanner)
    pub ports: Vec<u16>,
    /// Inode numbers of sockets owned by this process (for socket→process mapping)
    pub socket_inodes: Vec<u64>,
}

/// Scan /proc for all processes (or only Node/Bun if node_only=true).
/// Works on Linux. On macOS falls back to `ps`-based scanning.
pub fn scan_processes(node_only: bool) -> Result<Vec<ProcessInfo>> {
    #[cfg(target_os = "linux")]
    {
        scan_proc_linux(node_only)
    }
    #[cfg(target_os = "macos")]
    {
        scan_proc_macos(node_only)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(vec![])
    }
}

// ── Linux /proc scanning ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn scan_proc_linux(node_only: bool) -> Result<Vec<ProcessInfo>> {
    let mut results = Vec::new();

    for entry in fs::read_dir("/proc").context("read /proc")? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Only numeric directories are PIDs
        let name = entry.file_name();
        let pid_str = name.to_string_lossy();
        let pid: u32 = match pid_str.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        if let Some(info) = read_proc_entry(pid, node_only) {
            results.push(info);
        }
    }

    Ok(results)
}

#[cfg(target_os = "linux")]
fn read_proc_entry(pid: u32, node_only: bool) -> Option<ProcessInfo> {
    let proc_dir = format!("/proc/{}", pid);

    // Read comm (process name, truncated to 15 chars by kernel)
    let comm = fs::read_to_string(format!("{}/comm", proc_dir))
        .ok()?
        .trim()
        .to_string();

    let runtime = classify_runtime(&comm);

    if node_only && !matches!(runtime, Runtime::Node | Runtime::Bun) {
        return None;
    }

    // Read full cmdline for script path extraction
    let cmdline_raw = fs::read(format!("{}/cmdline", proc_dir)).ok()?;
    let args: Vec<String> = cmdline_raw
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect();

    // cwd symlink
    let cwd = fs::read_link(format!("{}/cwd", proc_dir)).ok();

    // Extract the script file from cmdline args
    let script = extract_script(&args, &runtime);

    let service_name = derive_service_name(&script, &cwd, &comm);

    // Collect socket inodes from /proc/<pid>/fd/
    let socket_inodes = collect_socket_inodes(pid);

    Some(ProcessInfo {
        pid,
        name: comm,
        runtime,
        script,
        cwd,
        service_name,
        ports: vec![],
        socket_inodes,
    })
}

#[cfg(target_os = "linux")]
fn collect_socket_inodes(pid: u32) -> Vec<u64> {
    let fd_dir = format!("/proc/{}/fd", pid);
    let mut inodes = Vec::new();

    let entries = match fs::read_dir(&fd_dir) {
        Ok(e) => e,
        Err(_) => return inodes,
    };

    for entry in entries.flatten() {
        if let Ok(target) = fs::read_link(entry.path()) {
            let t = target.to_string_lossy();
            // Socket symlinks look like: socket:[12345]
            if let Some(inode_str) = t.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
                if let Ok(inode) = inode_str.parse::<u64>() {
                    inodes.push(inode);
                }
            }
        }
    }

    inodes
}

// ── macOS ps-based scanning ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn scan_proc_macos(node_only: bool) -> Result<Vec<ProcessInfo>> {
    use std::process::Command;

    // ps output: pid, comm, args
    let output = Command::new("ps")
        .args(["-eo", "pid,comm,args"])
        .output()
        .context("run ps")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.trim_start().splitn(3, ' ').collect();
        if parts.len() < 2 {
            continue;
        }

        let pid: u32 = match parts[0].trim().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let comm = parts[1].trim();
        // comm may be full path; take the basename
        let basename = comm.split('/').last().unwrap_or(comm);

        let runtime = classify_runtime(basename);

        if node_only && !matches!(runtime, Runtime::Node | Runtime::Bun) {
            continue;
        }

        let full_cmd = parts.get(2).unwrap_or(&"").to_string();
        let args: Vec<String> = full_cmd.split_whitespace().map(String::from).collect();

        let cwd = read_cwd_macos(pid);
        let script = extract_script(&args, &runtime);
        let service_name = derive_service_name(&script, &cwd, basename);

        results.push(ProcessInfo {
            pid,
            name: basename.to_string(),
            runtime,
            script,
            cwd,
            service_name,
            ports: vec![],
            socket_inodes: vec![],
        });
    }

    // Populate listening ports for every process in one lsof call
    populate_ports_macos(&mut results);

    Ok(results)
}

/// Use a single `lsof -iTCP -sTCP:LISTEN -F pn` call to map every listening
/// port back to the process that owns it. Runs once per collector tick.
#[cfg(target_os = "macos")]
fn populate_ports_macos(processes: &mut Vec<ProcessInfo>) {
    use std::process::Command;

    // -F pn: parseable output — lines starting with 'p' are PIDs, 'n' are names
    // Example output:
    //   p12345
    //   n*:3000
    //   p67890
    //   n127.0.0.1:8080
    let out = match Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P", "-F", "pn"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };

    let text = String::from_utf8_lossy(&out.stdout);
    let mut current_pid: Option<u32> = None;

    for line in text.lines() {
        if let Some(pid_str) = line.strip_prefix('p') {
            current_pid = pid_str.parse().ok();
        } else if let Some(name) = line.strip_prefix('n') {
            // "name" is like "*:3000" or "127.0.0.1:3000" or "[::1]:3000"
            if let Some(port_str) = name.rsplit(':').next() {
                if let Ok(port) = port_str.trim_end_matches(']').parse::<u16>() {
                    if let Some(pid) = current_pid {
                        if let Some(p) = processes.iter_mut().find(|p| p.pid == pid) {
                            if !p.ports.contains(&port) {
                                p.ports.push(port);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn read_cwd_macos(pid: u32) -> Option<PathBuf> {
    use std::process::Command;
    // lsof can give us the cwd
    let out = Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(path) = line.strip_prefix('n') {
            return Some(PathBuf::from(path));
        }
    }
    None
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn classify_runtime(comm: &str) -> Runtime {
    let base = comm.split('/').last().unwrap_or(comm);
    match base {
        n if n.starts_with("node") => Runtime::Node,
        b if b.starts_with("bun") => Runtime::Bun,
        other => Runtime::Other(other.to_string()),
    }
}

/// Pull the first argument that looks like a JS/TS script file.
fn extract_script(args: &[String], runtime: &Runtime) -> Option<String> {
    // Skip argv[0] (the executable itself)
    for arg in args.iter().skip(1) {
        // Skip flags
        if arg.starts_with('-') {
            continue;
        }
        // Skip bun sub-commands (run, serve, install, …)
        if matches!(runtime, Runtime::Bun)
            && matches!(arg.as_str(), "run" | "serve" | "dev" | "install" | "add" | "remove" | "test")
        {
            continue;
        }
        // Accept anything that looks like a JS/TS file or a bare module path
        if arg.ends_with(".js")
            || arg.ends_with(".mjs")
            || arg.ends_with(".cjs")
            || arg.ends_with(".ts")
            || arg.ends_with(".tsx")
            || (!arg.contains('=') && !arg.starts_with('/') && !arg.is_empty())
        {
            return Some(arg.clone());
        }
    }
    None
}

/// Build a human-friendly service name from available info.
fn derive_service_name(script: &Option<String>, cwd: &Option<PathBuf>, fallback: &str) -> String {
    // Prefer the script stem (e.g. "server" from "server.js")
    if let Some(s) = script {
        let stem = PathBuf::from(s)
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| s.clone());
        return stem;
    }
    // Fall back to the project folder name
    if let Some(dir) = cwd {
        if let Some(folder) = dir.file_name() {
            return folder.to_string_lossy().to_string();
        }
    }
    fallback.to_string()
}
