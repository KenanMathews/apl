use crate::core::config::agent_home;
/// AGENT.SYS — System execution
///
/// Runs commands as the agent user using std::process::Command.
/// This is the core shell-out pattern — Rust orchestrates,
/// the OS does the actual work.

use std::process::Command as SysCmd;
use std::time::{Duration, Instant};
use crate::core::{parser::Command, result::AplResult};


const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// AGENT.SYS.run("cmd", cwd="workspace", timeout=30, shell=false)
pub fn run(cmd: &Command) -> AplResult {
    let command_str = match cmd.pos_str(0) {
        Some(c) => c,
        None => return AplResult::err("run() requires a command argument"),
    };

    let cwd = cmd.named_str_default("cwd", "workspace");
    let timeout = cmd.named_int_default("timeout", DEFAULT_TIMEOUT_SECS as i64) as u64;
    let use_shell = cmd.named_bool_default("shell", false);

    // Resolve working directory
    let work_dir = if cwd.starts_with('/') {
        std::path::PathBuf::from(&cwd)
    } else {
        std::path::PathBuf::from(agent_home()).join(&cwd)
    };

    if !work_dir.exists() {
        return AplResult::err(format!("working directory not found: {}", cwd));
    }

    let start = Instant::now();

    // Build the command
    let output = if use_shell {
        SysCmd::new("sh")
            .arg("-c")
            .arg(&command_str)
            .current_dir(&work_dir)
            .env("HOME", &agent_home())
            .env("USER", "agent")
            .output()
    } else {
        // Split command into program + args
        let mut parts = shell_split(&command_str);
        if parts.is_empty() {
            return AplResult::err("empty command");
        }
        let program = parts.remove(0);

        SysCmd::new(&program)
            .args(&parts)
            .current_dir(&work_dir)
            .env("HOME", &agent_home())
            .env("USER", "agent")
            .output()
    };

    let elapsed_ms = start.elapsed().as_millis();

    // Check timeout (basic — for true timeout we'd use threads)
    if elapsed_ms > (timeout * 1000) as u128 {
        return AplResult::err_meta(
            vec![("time", format!("{}ms", elapsed_ms))],
            format!("command timed out after {}s", timeout),
        );
    }

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code().unwrap_or(-1);

            if out.status.success() {
                let mut result = AplResult::ok_meta(vec![
                    ("exit", exit_code.to_string()),
                    ("time", format!("{}ms", elapsed_ms)),
                ]);
                result.out = if stdout.trim().is_empty() {
                    Some("—".to_string())
                } else {
                    Some(stdout.trim_end().to_string())
                };
                result.err = if stderr.trim().is_empty() {
                    Some("—".to_string())
                } else {
                    Some(stderr.trim_end().to_string())
                };
                result
            } else {
                AplResult::err_meta(
                    vec![
                        ("exit", exit_code.to_string()),
                        ("time", format!("{}ms", elapsed_ms)),
                    ],
                    if stderr.trim().is_empty() {
                        format!("command exited with code {}", exit_code)
                    } else {
                        stderr.trim().to_string()
                    },
                )
                .with_out(if stdout.trim().is_empty() {
                    "—".to_string()
                } else {
                    stdout.trim_end().to_string()
                })
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::err(format!("command not found: {}", command_str.split_whitespace().next().unwrap_or("")))
        }
        Err(e) => AplResult::err(format!("execution error: {}", e)),
    }
}

/// AGENT.SYS.capabilities()
/// Shells out to check what's available — no native Rust needed
pub fn capabilities(_cmd: &Command) -> AplResult {
    let mut lines = Vec::new();

    // OS info
    let os = read_file("/etc/os-release")
        .lines()
        .find(|l| l.starts_with("PRETTY_NAME="))
        .and_then(|l| l.split('=').nth(1))
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".to_string());

    lines.push(format!("  os:        {}", os));

    // Python version
    let python = shell_output("python3 --version")
        .map(|s| s.trim().replace("Python ", ""))
        .unwrap_or_else(|| "not installed".to_string());
    lines.push(format!("  python:    {}", python));

    // Memory
    let (total_mb, free_mb) = read_mem_info();
    lines.push(format!("  ram:       {}mb free of {}mb", free_mb, total_mb));

    // Disk
    let (total_gb, free_gb) = disk_info(agent_home().as_str());
    lines.push(format!("  disk:      {}gb free of {}gb", free_gb, total_gb));

    // CPU cores
    let cores = shell_output("nproc")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "?".to_string());
    lines.push(format!("  cpu:       {} cores", cores));

    // Check useful tools
    let tools = [
        "python3", "node", "npm", "git", "curl", "wget",
        "ffmpeg", "pandoc", "jq", "sqlite3", "make",
        "xdotool", "ydotool", "notify-send", "scrot", "grim",
        "xdg-open", "busctl", "dbus-monitor",
    ];

    let installed: Vec<&str> = tools.iter()
        .filter(|t| which(t))
        .copied()
        .collect();

    lines.push(format!("  installed: {}", installed.join(" ")));

    let mut result = AplResult::ok();
    result.out = Some(lines.join("\n"));
    result
}

/// AGENT.SYS.env("KEY")
pub fn env(cmd: &Command) -> AplResult {
    let key = match cmd.pos_str(0) {
        Some(k) => k,
        None => return AplResult::err("env() requires a key argument"),
    };

    match std::env::var(&key) {
        Ok(val) => AplResult::ok_meta(vec![("key", key)]).with_val(val),
        Err(_) => AplResult::err(format!("variable not set: {}", key)),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check if a command exists on PATH
fn which(cmd: &str) -> bool {
    SysCmd::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a shell command and return stdout as String
fn shell_output(cmd: &str) -> Option<String> {
    let parts = shell_split(cmd);
    if parts.is_empty() { return None; }
    SysCmd::new(&parts[0])
        .args(&parts[1..])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Read a file to string, returning empty string on error
fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Parse /proc/meminfo for total and free MB
fn read_mem_info() -> (u64, u64) {
    let content = read_file("/proc/meminfo");
    let mut total = 0u64;
    let mut free = 0u64;
    let mut available = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_kb(line);
        } else if line.starts_with("MemFree:") {
            free = parse_kb(line);
        } else if line.starts_with("MemAvailable:") {
            available = parse_kb(line);
        }
    }

    let free_mb = if available > 0 { available / 1024 } else { free / 1024 };
    (total / 1024, free_mb)
}

fn parse_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Get disk info using df
fn disk_info(path: &str) -> (u64, u64) {
    let output = SysCmd::new("df")
        .arg("-BG")
        .arg("--output=size,avail")
        .arg(path)
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        if let Some(line) = text.lines().nth(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let total = parts[0].trim_end_matches('G').parse().unwrap_or(0);
                let free = parts[1].trim_end_matches('G').parse().unwrap_or(0);
                return (total, free);
            }
        }
    }
    (0, 0)
}

/// Very simple shell word splitter — handles quoted strings
fn shell_split(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    for ch in s.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// Route SYS commands
pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "run"          => run(cmd),
        "capabilities" => capabilities(cmd),
        "env"          => env(cmd),
        other => AplResult::err(format!("unknown SYS command: {}", other)),
    }
}
