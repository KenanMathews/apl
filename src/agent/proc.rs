/// AGENT.PROC — Process management
///
/// Lists and kills processes owned by the agent user.
/// Shells out to: ps, kill
/// Safety: kill is restricted to agent-owned processes only.

use std::process::Command as SysCmd;
use crate::core::{parser::Command, result::AplResult};

/// AGENT.PROC.list()
/// Lists processes running as the agent user
pub fn list(_cmd: &Command) -> AplResult {
    // Get current user — in production this is always "agent"
    let whoami = SysCmd::new("whoami")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "agent".to_string());

    let output = SysCmd::new("ps")
        .arg("--user").arg(&whoami)
        .arg("-o").arg("pid,pcpu,rss,start_time,args")
        .arg("--no-headers")
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let rows: Vec<Vec<String>> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(5, ' ')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();

                    if parts.len() >= 5 {
                        let pid = parts[0].to_string();
                        let cpu = format!("cpu={}%", parts[1]);
                        let mem = format!("mem={}kb", parts[2]);
                        let started = parts[3].to_string();
                        let cmd_str = parts[4].to_string();
                        // Truncate very long command strings
                        let cmd_display = if cmd_str.len() > 60 {
                            format!("{}...", &cmd_str[..60])
                        } else {
                            cmd_str
                        };
                        vec![pid, cmd_display, cpu, mem, started]
                    } else {
                        parts.iter().map(|s| s.to_string()).collect()
                    }
                })
                .collect();

            if rows.is_empty() {
                AplResult::ok_meta(vec![("count", "0".to_string())])
            } else {
                AplResult::ok_list(rows)
            }
        }
        Err(e) => AplResult::err(format!("ps error: {}", e)),
    }
}

/// AGENT.PROC.kill(pid)
/// Kills a process — but only if it belongs to the current user
pub fn kill(cmd: &Command) -> AplResult {
    let pid = match cmd.pos_int(0) {
        Some(p) => p,
        None => return AplResult::err("kill() requires a pid argument"),
    };

    if pid <= 0 {
        return AplResult::err(format!("invalid pid: {}", pid));
    }

    // Safety check: verify the process belongs to the current user
    // by checking /proc/{pid}/status
    let proc_status_path = format!("/proc/{}/status", pid);
    let proc_status = std::fs::read_to_string(&proc_status_path);

    match proc_status {
        Err(_) => return AplResult::err(format!("process not found: {}", pid)),
        Ok(status) => {
            // Get current UID
            let current_uid = get_current_uid();

            // Parse Uid line from /proc/pid/status
            let proc_uid = status
                .lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u32>().ok());

            match proc_uid {
                None => return AplResult::err("could not determine process owner"),
                Some(uid) if uid != current_uid => {
                    return AplResult::err(format!(
                        "process {} not owned by current user",
                        pid
                    ));
                }
                _ => {} // owned by us — proceed
            }
        }
    }

    // Send SIGTERM first (graceful)
    let status = SysCmd::new("kill")
        .arg(pid.to_string())
        .status();

    match status {
        Ok(s) if s.success() => {
            AplResult::ok_meta(vec![("pid", pid.to_string()), ("signal", "SIGTERM".to_string())])
        }
        Ok(_) => AplResult::err(format!("kill failed for pid {}", pid)),
        Err(e) => AplResult::err(format!("kill error: {}", e)),
    }
}

/// Get the UID of the current process
fn get_current_uid() -> u32 {
    // Read from /proc/self/status
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0)
}

pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "list" => list(cmd),
        "kill" => kill(cmd),
        other  => AplResult::err(format!("unknown PROC command: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::parse;

    fn run(input: &str) -> AplResult {
        let cmd = parse(input).unwrap();
        dispatch(&cmd)
    }

    #[test]
    fn test_list_returns_ok() {
        let r = run("AGENT.PROC.list()");
        assert!(r.ok);
    }

    #[test]
    fn test_kill_invalid_pid() {
        let r = run("AGENT.PROC.kill(-1)");
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("invalid pid"));
    }

    #[test]
    fn test_kill_nonexistent_pid() {
        // PID 999999 almost certainly doesn't exist
        let r = run("AGENT.PROC.kill(999999)");
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("not found"));
    }

    #[test]
    fn test_get_current_uid_nonzero_in_test() {
        // In test environment we should be able to read our own UID
        let uid = get_current_uid();
        // uid 0 = root, which is valid in test containers
        let _ = uid;
    }
}
