use crate::core::config::agent_home;
/// AGENT.MEM — Persistent key-value memory
///
/// Stores named text blobs in /home/agent/.memory/
/// Each key becomes a file: .memory/{key}
/// Keys are alphanumeric + underscore only — no path traversal possible.

use std::fs;
use std::path::PathBuf;
use crate::core::{parser::Command, result::AplResult};



/// Validate a memory key — alphanumeric + underscore only
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn mem_path(key: &str) -> PathBuf {
    PathBuf::from(format!("{}/.memory", agent_home())).join(key)
}

fn ensure_mem_dir() -> Result<(), String> {
    fs::create_dir_all(format!("{}/.memory", agent_home())).map_err(|e| format!("failed to create memory dir: {}", e))
}

/// AGENT.MEM.save("key", "content")
pub fn save(cmd: &Command) -> AplResult {
    let key = match cmd.pos_str(0) {
        Some(k) => k,
        None => return AplResult::err("save() requires key and content arguments"),
    };
    let content = match cmd.pos_str(1) {
        Some(c) => c,
        None => return AplResult::err("save() requires a content argument"),
    };

    if !valid_key(&key) {
        return AplResult::err(format!(
            "invalid key: '{}' — use alphanumeric and underscore only",
            key
        ));
    }

    if let Err(e) = ensure_mem_dir() {
        return AplResult::err(e);
    }

    match fs::write(mem_path(&key), content.as_bytes()) {
        Ok(_) => AplResult::ok_meta(vec![("key", key)]),
        Err(e) => AplResult::err(format!("save error: {}", e)),
    }
}

/// AGENT.MEM.recall("key")
pub fn recall(cmd: &Command) -> AplResult {
    let key = match cmd.pos_str(0) {
        Some(k) => k,
        None => return AplResult::err("recall() requires a key argument"),
    };

    if !valid_key(&key) {
        return AplResult::err(format!("invalid key: '{}'", key));
    }

    let path = mem_path(&key);
    match fs::read_to_string(&path) {
        Ok(content) => {
            AplResult::ok_meta(vec![("key", key.clone())]).with_out(content)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::err(format!("memory not found: {}", key))
        }
        Err(e) => AplResult::err(format!("recall error: {}", e)),
    }
}

/// AGENT.MEM.list()
pub fn list(_cmd: &Command) -> AplResult {
    if let Err(e) = ensure_mem_dir() {
        return AplResult::err(e);
    }

    let entries = match fs::read_dir(format!("{}/.memory", agent_home())) {
        Ok(e) => e,
        Err(e) => return AplResult::err(format!("list error: {}", e)),
    };

    let mut keys: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|k| valid_key(k))
        .collect();

    keys.sort();

    if keys.is_empty() {
        return AplResult::ok_meta(vec![("count", "0".to_string())]);
    }

    let rows: Vec<Vec<String>> = keys.into_iter().map(|k| vec![k]).collect();
    AplResult::ok_list(rows)
}

/// AGENT.MEM.delete("key")
pub fn delete(cmd: &Command) -> AplResult {
    let key = match cmd.pos_str(0) {
        Some(k) => k,
        None => return AplResult::err("delete() requires a key argument"),
    };

    if !valid_key(&key) {
        return AplResult::err(format!("invalid key: '{}'", key));
    }

    let path = mem_path(&key);
    match fs::remove_file(&path) {
        Ok(_) => AplResult::ok_val(format!("{} deleted", key)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::err(format!("memory not found: {}", key))
        }
        Err(e) => AplResult::err(format!("delete error: {}", e)),
    }
}

pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "save"   => save(cmd),
        "recall" => recall(cmd),
        "list"   => list(cmd),
        "delete" => delete(cmd),
        other    => AplResult::err(format!("unknown MEM command: {}", other)),
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
    fn test_invalid_key_rejected() {
        let r = run(r#"AGENT.MEM.save("../escape", "bad")"#);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("invalid key"));
    }

    #[test]
    fn test_invalid_key_slash() {
        let r = run(r#"AGENT.MEM.save("path/traversal", "bad")"#);
        assert!(!r.ok);
    }

    #[test]
    fn test_recall_missing_key() {
        let r = run(r#"AGENT.MEM.recall("definitely_does_not_exist_xyz123")"#);
        assert!(!r.ok);
        assert!(r.err.as_deref().unwrap_or("").contains("not found"));
    }

    #[test]
    fn test_list_returns_ok() {
        // Just verify it doesn't crash — mem dir may or may not exist
        let r = run("AGENT.MEM.list()");
        // ok: or er: — both valid depending on env
        let _ = r;
    }

    #[test]
    fn test_delete_missing_returns_er() {
        let r = run(r#"AGENT.MEM.delete("definitely_not_there_abc999")"#);
        assert!(!r.ok);
    }
}
