use crate::core::config::agent_home;
/// AGENT.ESCALATE — Hand back to the human
///
/// Writes a request to /home/agent/inbox/escalate.pending
/// and waits for the session bridge to pick it up,
/// show the user a dialog, and write the response to
/// /home/agent/inbox/escalate.response
///
/// The session bridge handles the actual dialog display.
/// Here we just write the request and poll for the response.

use std::fs;
use std::thread;
use std::time::{Duration, Instant};
use crate::core::{parser::{Command, Arg, Value}, result::AplResult};



const POLL_INTERVAL_MS: u64 = 200;
const TIMEOUT_SECS: u64    = 300; // 5 minutes

/// AGENT.ESCALATE("reason", options=["a", "b", "c"])
fn pending_path()  -> String { format!("{}/inbox/escalate.pending",  agent_home()) }
fn response_path() -> String { format!("{}/inbox/escalate.response", agent_home()) }

pub fn escalate(cmd: &Command) -> AplResult {
    let reason = match cmd.pos_str(0) {
        Some(r) => r,
        None    => return AplResult::err("ESCALATE() requires a reason argument"),
    };

    // Collect options if provided
    let options: Vec<String> = cmd.args.iter()
        .find_map(|a| {
            if let Arg::Named(k, Value::List(items)) = a {
                if k == "options" {
                    return Some(
                        items.iter()
                            .filter_map(|v| v.as_str_owned())
                            .collect()
                    );
                }
            }
            None
        })
        .unwrap_or_default();

    // Ensure inbox exists
    if let Err(e) = fs::create_dir_all("/home/agent/inbox") {
        return AplResult::err(format!("failed to create inbox: {}", e));
    }

    // Remove any stale response from a previous escalation
    let _ = fs::remove_file(&response_path());

    // Write the pending escalation as a simple text format
    let mut pending = format!("reason: {}\n", reason);
    if !options.is_empty() {
        pending.push_str(&format!("options: {}\n", options.join("|")));
    }

    if let Err(e) = fs::write(&pending_path(), &pending) {
        return AplResult::err(format!("failed to write escalation: {}", e));
    }

    // Poll for response
    let start = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));

        if start.elapsed().as_secs() >= TIMEOUT_SECS {
            let _ = fs::remove_file(&pending_path());
            return AplResult::err("escalation timed out — no response after 5 minutes");
        }

        match fs::read_to_string(&response_path()) {
            Ok(response) => {
                let response = response.trim().to_string();
                // Clean up both files
                let _ = fs::remove_file(&pending_path());
                let _ = fs::remove_file(&response_path());

                if response == "__dismissed__" {
                    return AplResult::err("user dismissed dialog without responding");
                }

                return AplResult::ok_val(response);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Not ready yet — keep polling
                continue;
            }
            Err(e) => {
                let _ = fs::remove_file(&pending_path());
                return AplResult::err(format!("escalation read error: {}", e));
            }
        }
    }
}

pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "escalate" => escalate(cmd),
        other      => AplResult::err(format!("unknown ESCALATE command: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::parse;

    #[test]
    fn test_missing_reason_rejected() {
        let cmd = parse("AGENT.ESCALATE()").unwrap();
        let r = dispatch(&cmd);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("reason"));
    }
}
