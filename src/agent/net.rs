/// AGENT.NET — Outbound HTTP only
///
/// Uses std::process::Command to shell out to curl.
/// curl is available on virtually every Linux system.
/// No reqwest dependency needed — simpler and lighter.

use std::process::Command as SysCmd;
use crate::core::{parser::{Command, Arg, Value}, result::AplResult};

/// AGENT.NET.fetch(url, method="GET", headers={}, body=null, timeout=30)
pub fn fetch(cmd: &Command) -> AplResult {
    let url = match cmd.pos_str(0) {
        Some(u) => u,
        None => return AplResult::err("fetch() requires a url argument"),
    };

    // Basic URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return AplResult::err(format!("invalid url — must start with http:// or https://: {}", url));
    }

    let method = cmd.named_str_default("method", "GET").to_uppercase();
    let body = cmd.named_str("body");
    let timeout = cmd.named_int_default("timeout", 30);

    // Check curl is available
    let curl_check = SysCmd::new("which").arg("curl").output();
    if curl_check.map(|o| !o.status.success()).unwrap_or(true) {
        return AplResult::err("curl not found — install curl");
    }

    let mut curl = SysCmd::new("curl");

    // Silent but show errors
    curl.arg("--silent");
    curl.arg("--show-error");

    // Include response headers in a separate write
    curl.arg("--write-out").arg("\n---STATUS:%{http_code}---TIME:%{time_total}---");

    // Method
    curl.arg("--request").arg(&method);

    // Timeout
    curl.arg("--max-time").arg(timeout.to_string());

    // Follow redirects
    curl.arg("--location");

    // Headers from named arg — headers={"key": "value"}
    // We look for named args that match header-like patterns
    for arg in &cmd.args {
        if let Arg::Named(k, Value::Str(v)) = arg {
            if k == "content_type" || k == "Content-Type" {
                curl.arg("--header").arg(format!("Content-Type: {}", v));
            } else if k == "authorization" || k == "Authorization" {
                curl.arg("--header").arg(format!("Authorization: {}", v));
            } else if k.starts_with("header_") {
                let header_name = k.trim_start_matches("header_").replace('_', "-");
                curl.arg("--header").arg(format!("{}: {}", header_name, v));
            }
        }
    }

    // Body
    if let Some(b) = &body {
        curl.arg("--data").arg(b);
    }

    curl.arg(&url);

    let start = std::time::Instant::now();
    let output = curl.output();
    let elapsed_ms = start.elapsed().as_millis();

    match output {
        Ok(out) => {
            let full = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            if !out.status.success() && full.is_empty() {
                return AplResult::err_meta(
                    vec![("time", format!("{}ms", elapsed_ms))],
                    stderr.trim().to_string(),
                );
            }

            // Parse out the status code we embedded with --write-out
            let (body_part, status_code, _curl_time) = parse_curl_output(&full);

            if status_code >= 400 {
                AplResult::err_meta(
                    vec![
                        ("status", status_code.to_string()),
                        ("time", format!("{}ms", elapsed_ms)),
                    ],
                    format!("HTTP {} error", status_code),
                )
                .with_out(body_part)
            } else {
                let mut result = AplResult::ok_meta(vec![
                    ("status", status_code.to_string()),
                    ("time", format!("{}ms", elapsed_ms)),
                ]);
                result.out = Some(truncate_body(&body_part, 4096));
                result
            }
        }
        Err(e) => AplResult::err_meta(
            vec![("time", format!("{}ms", elapsed_ms))],
            format!("curl error: {}", e),
        ),
    }
}

/// Parse the curl output that has our injected status marker
fn parse_curl_output(raw: &str) -> (String, u16, f64) {
    if let Some(marker_pos) = raw.rfind("\n---STATUS:") {
        let body = raw[..marker_pos].to_string();
        let marker = &raw[marker_pos + 1..];

        let status: u16 = marker
            .split("---STATUS:")
            .nth(1)
            .and_then(|s| s.split("---").next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let time: f64 = marker
            .split("---TIME:")
            .nth(1)
            .and_then(|s| s.split("---").next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        (body, status, time)
    } else {
        (raw.to_string(), 0, 0.0)
    }
}

/// Truncate large bodies with a note
fn truncate_body(body: &str, max: usize) -> String {
    if body.len() <= max {
        body.to_string()
    } else {
        format!(
            "{}\n... (truncated — {} bytes total)",
            &body[..max],
            body.len()
        )
    }
}

pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "fetch" => fetch(cmd),
        other   => AplResult::err(format!("unknown NET command: {}", other)),
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
    fn test_invalid_url_rejected() {
        let r = run(r#"AGENT.NET.fetch("ftp://bad-scheme.com")"#);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("invalid url"));
    }

    #[test]
    fn test_missing_url_rejected() {
        let r = run("AGENT.NET.fetch()");
        assert!(!r.ok);
    }

    #[test]
    fn test_parse_curl_output_extracts_status() {
        let raw = "hello world\n---STATUS:200---TIME:0.123---";
        let (body, status, _) = parse_curl_output(raw);
        assert_eq!(body, "hello world");
        assert_eq!(status, 200);
    }

    #[test]
    fn test_truncate_body_short() {
        let s = "hello";
        assert_eq!(truncate_body(s, 100), "hello");
    }

    #[test]
    fn test_truncate_body_long() {
        let s = "x".repeat(5000);
        let t = truncate_body(&s, 100);
        assert!(t.contains("truncated"));
        assert!(t.len() < 5000);
    }
}
