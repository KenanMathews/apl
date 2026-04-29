mod core {
    pub mod config;
    pub mod parser;
    pub mod result;
    pub mod server;
}

mod agent {
    pub mod fs;
    pub mod sys;
    pub mod mem;
    pub mod net;
    pub mod proc;
    pub mod log;
    pub mod escalate;
}

mod session;

use core::parser::parse;
use core::result::AplResult;
use core::server::Server;

/// Top-level dispatcher — routes a parsed command to the right handler
pub fn dispatch(input: &str) -> AplResult {
    match parse(input) {
        Err(e) => AplResult::err(format!("parse error: {}", e)),
        Ok(cmd) => match cmd.namespace.as_str() {
            "AGENT" => match cmd.subspace.as_str() {
                "FS"       => agent::fs::dispatch(&cmd),
                "SYS"      => agent::sys::dispatch(&cmd),
                "MEM"      => agent::mem::dispatch(&cmd),
                "NET"      => agent::net::dispatch(&cmd),
                "PROC"     => agent::proc::dispatch(&cmd),
                "LOG"      => agent::log::dispatch(&cmd),
                ""         => agent::escalate::dispatch(&cmd), // AGENT.ESCALATE
                _          => AplResult::err(format!(
                    "unknown AGENT subspace: {}",
                    cmd.subspace
                )),
            },
            "SESSION" => {
                // Route extended commands (ask, watch) separately
                match cmd.action.as_str() {
                    "ask" | "watch" | "watch.stop" => {
                        session::extended::dispatch_extended(&cmd)
                    }
                    _ => session::dispatch(&cmd),
                }
            }
            other => AplResult::err(format!("unknown namespace: {}", other)),
        },
    }
}

fn main() {
    use std::env;

    let args: Vec<String> = env::args().collect();

    // apl --daemon  → run as Unix socket server
    // apl           → REPL mode (stdin/stdout)
    // apl "AGENT.FS.read(...)"  → single command mode

    if args.len() > 1 && args[1] == "--daemon" {
        let socket_path = args.get(2)
            .map(|s| s.as_str())
            .unwrap_or(core::server::SOCKET_PATH);

        eprintln!("apl v0.1 starting in daemon mode");
        let server = Server::new(socket_path);
        server.run(dispatch);

    } else if args.len() > 1 {
        // Single command mode: apl "AGENT.FS.list()"
        let cmd = args[1..].join(" ");
        let result = dispatch(&cmd);
        println!("{}", result.render());

    } else {
        // REPL mode
        use std::io::{self, BufRead, Write};
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut out = stdout.lock();

        eprintln!("APL v0.1 — enter commands (Ctrl+D to exit):");

        for line in stdin.lock().lines() {
            match line {
                Ok(input) => {
                    let input = input.trim().to_string();
                    if input.is_empty() || input.starts_with('#') { continue; }
                    let result = dispatch(&input);
                    writeln!(out, "{}\n", result.render()).ok();
                }
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_returns_er() {
        let r = dispatch("not valid");
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("parse error"));
    }

    #[test]
    fn test_unknown_subspace_returns_er() {
        let r = dispatch(r#"AGENT.UNKNOWN.foo("bar")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_agent_fs_exists_dispatches() {
        // Path inside agent home that may or may not exist — just check ok: or er: parses
        let r = dispatch(r#"AGENT.FS.exists("workspace")"#);
        // Sandbox will reject /tmp — workspace may not exist but should still parse cleanly
        let rendered = r.render();
        assert!(rendered.starts_with("ok:") || rendered.starts_with("er:"));
    }

    #[test]
    fn test_agent_mem_invalid_key() {
        let r = dispatch(r#"AGENT.MEM.recall("../escape")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_agent_net_bad_url() {
        let r = dispatch(r#"AGENT.NET.fetch("ftp://bad")"#);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("invalid url"));
    }

    #[test]
    fn test_agent_proc_list() {
        let r = dispatch("AGENT.PROC.list()");
        assert!(r.ok);
    }

    #[test]
    fn test_agent_log_invalid_level() {
        let r = dispatch(r#"AGENT.LOG.write("verbose", "msg")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_session_watch_bad_target() {
        let r = dispatch(r#"SESSION.watch("badtarget")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_agent_sys_capabilities() {
        let r = dispatch("AGENT.SYS.capabilities()");
        assert!(r.ok);
        assert!(r.out.as_deref().unwrap_or("").contains("os:"));
    }

    #[test]
    fn test_agent_proc_kill_invalid() {
        let r = dispatch("AGENT.PROC.kill(999999)");
        assert!(!r.ok);
    }

    #[test]
    fn test_agent_escalate_no_reason() {
        let r = dispatch("AGENT.ESCALATE()");
        assert!(!r.ok);
    }

    #[test]
    fn test_full_render_format_ok() {
        let r = dispatch("AGENT.PROC.list()");
        let rendered = r.render();
        assert!(rendered.starts_with("ok:") || rendered.starts_with("er:"));
    }

    #[test]
    fn test_full_render_format_er() {
        let r = dispatch(r#"AGENT.NET.fetch("ftp://bad")"#);
        let rendered = r.render();
        assert!(rendered.starts_with("er:"));
        assert!(rendered.contains("err:"));
    }
}
