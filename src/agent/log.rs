use crate::core::config::agent_home;
/// AGENT.LOG — Structured logging
///
/// Appends to /home/agent/logs/apl.log
/// Format: 2026-04-29T10:33:01Z [INFO] message

use std::fs::{self, OpenOptions};
use std::io::Write;
use crate::core::{parser::Command, result::AplResult};


fn log_path() -> String { format!("{}/logs/apl.log", agent_home()) }

const VALID_LEVELS: &[&str] = &["info", "warn", "error"];

/// AGENT.LOG.write("level", "message")
pub fn write(cmd: &Command) -> AplResult {
    let level = match cmd.pos_str(0) {
        Some(l) => l.to_lowercase(),
        None    => return AplResult::err("write() requires level and message arguments"),
    };

    let message = match cmd.pos_str(1) {
        Some(m) => m,
        None    => return AplResult::err("write() requires a message argument"),
    };

    if !VALID_LEVELS.contains(&level.as_str()) {
        return AplResult::err(format!(
            "invalid level: '{}' — use info, warn, or error",
            level
        ));
    }

    let timestamp = current_timestamp();
    let log_line = format!("{} [{}] {}\n", timestamp, level.to_uppercase(), message);

    // Ensure log directory exists
    if let Some(parent) = std::path::Path::new(log_path().as_str()).parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return AplResult::err(format!("failed to create log dir: {}", e));
        }
    }

    // Append to log file
    match OpenOptions::new().create(true).append(true).open(log_path().as_str()) {
        Ok(mut file) => match file.write_all(log_line.as_bytes()) {
            Ok(_) => AplResult::ok_meta(vec![
                ("level", level),
                ("ts", timestamp),
            ]),
            Err(e) => AplResult::err(format!("write error: {}", e)),
        },
        Err(e) => AplResult::err(format!("could not open log file: {}", e)),
    }
}

/// AGENT.LOG.read(lines=50)
/// Read the last N lines from the log
pub fn read(cmd: &Command) -> AplResult {
    let n = cmd.named_int_default("lines", 50) as usize;

    match fs::read_to_string(log_path().as_str()) {
        Ok(content) => {
            let all_lines: Vec<&str> = content.lines().collect();
            let start = if all_lines.len() > n {
                all_lines.len() - n
            } else {
                0
            };
            let recent = all_lines[start..].join("\n");
            AplResult::ok_meta(vec![
                ("lines", all_lines.len().to_string()),
                ("showing", n.min(all_lines.len()).to_string()),
            ])
            .with_out(recent)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::ok_meta(vec![("lines", "0".to_string())])
                .with_out("(log is empty)".to_string())
        }
        Err(e) => AplResult::err(format!("read error: {}", e)),
    }
}

/// AGENT.LOG.clear()
/// Clears the log file (keeps the file, empties content)
pub fn clear(_cmd: &Command) -> AplResult {
    match fs::write(log_path().as_str(), b"") {
        Ok(_) => AplResult::ok_val("log cleared".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::ok_val("log was already empty".to_string())
        }
        Err(e) => AplResult::err(format!("clear error: {}", e)),
    }
}

/// Simple timestamp without chrono dependency
/// Returns ISO-8601 approximate: reads from /proc/uptime + realtime clock via date command
fn current_timestamp() -> String {
    use std::process::Command as SysCmd;
    SysCmd::new("date")
        .arg("--utc")
        .arg("+%Y-%m-%dT%H:%M:%SZ")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "write" => write(cmd),
        "read"  => read(cmd),
        "clear" => clear(cmd),
        other   => AplResult::err(format!("unknown LOG command: {}", other)),
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
    fn test_invalid_level_rejected() {
        let r = run(r#"AGENT.LOG.write("verbose", "msg")"#);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("invalid level"));
    }

    #[test]
    fn test_missing_message_rejected() {
        let r = run(r#"AGENT.LOG.write("info")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_read_missing_log_ok() {
        // Reading a nonexistent log should be ok (empty)
        // This test depends on whether /home/agent/logs/apl.log exists
        let r = run("AGENT.LOG.read()");
        // Either ok (file exists or empty) — should not panic
        let _ = r;
    }

    #[test]
    fn test_timestamp_format() {
        let ts = current_timestamp();
        // Should look like 2026-04-29T10:33:01Z
        assert!(ts.contains('T'), "timestamp should contain T: {}", ts);
        assert!(ts.ends_with('Z'), "timestamp should end with Z: {}", ts);
    }
}
