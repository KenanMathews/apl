use crate::core::config::agent_home;
/// AGENT.FS — Filesystem operations
///
/// All paths are sandboxed to /home/agent/
/// Any attempt to escape via ../ or absolute path outside home is rejected.

use std::fs;
use std::path::{Path, PathBuf};
use crate::core::{parser::Command, result::AplResult};



/// Resolve and validate a path — must stay inside AGENT_HOME
fn safe_path(relative: &str) -> Result<PathBuf, String> {
    let base = PathBuf::from(agent_home());

    // Handle absolute paths — must start with agent home
    let candidate = if relative.starts_with('/') {
        PathBuf::from(relative)
    } else {
        base.join(relative)
    };

    // Canonicalize to resolve ../ traversal
    // If file doesn't exist yet, canonicalize the parent instead
    let resolved = if candidate.exists() {
        candidate.canonicalize().map_err(|e| e.to_string())?
    } else {
        // For files that don't exist yet (writes), check parent
        let parent = candidate.parent().unwrap_or(&candidate);
        if parent.exists() {
            let canon_parent = parent.canonicalize().map_err(|e| e.to_string())?;
            canon_parent.join(candidate.file_name().unwrap_or_default())
        } else {
            // Parent doesn't exist either — we'll create it, but check the base
            candidate.clone()
        }
    };

    // Verify it's inside agent home
    let canon_base = PathBuf::from(agent_home());
    if !resolved.starts_with(&canon_base) {
        return Err(format!(
            "permission denied: path outside agent home: {}",
            relative
        ));
    }

    Ok(resolved)
}

/// AGENT.FS.read("path")
pub fn read(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("read() requires a path argument"),
    };

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    match fs::read_to_string(&path) {
        Ok(content) => AplResult::ok_out(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AplResult::err(format!("file not found: {}", path_str))
        }
        Err(e) => AplResult::err(format!("read error: {}", e)),
    }
}

/// AGENT.FS.write("path", "content", append=false)
pub fn write(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("write() requires path and content arguments"),
    };

    let content = match cmd.pos_str(1) {
        Some(c) => c,
        None => return AplResult::err("write() requires a content argument"),
    };

    let append = cmd.named_bool_default("append", false);

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return AplResult::err(format!("failed to create directories: {}", e));
        }
    }

    let result = if append {
        use std::io::Write;
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(content.as_bytes()))
    } else {
        fs::write(&path, content.as_bytes())
    };

    match result {
        Ok(_) => {
            let bytes = content.len();
            AplResult::ok_meta(vec![("bytes", bytes.to_string())])
                .with_val(path.display().to_string())
        }
        Err(e) => AplResult::err(format!("write error: {}", e)),
    }
}

/// AGENT.FS.list("path", recursive=false)
pub fn list(cmd: &Command) -> AplResult {
    let path_str = cmd.pos_str(0).unwrap_or_else(|| "workspace".to_string());
    let recursive = cmd.named_bool_default("recursive", false);

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    if !path.exists() {
        return AplResult::err(format!("path not found: {}", path_str));
    }

    if !path.is_dir() {
        return AplResult::err(format!("not a directory: {}", path_str));
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    collect_entries(&path, &path, recursive, &mut rows);

    AplResult::ok_list(rows)
}

fn collect_entries(base: &Path, dir: &Path, recursive: bool, rows: &mut Vec<Vec<String>>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let path = entry.path();
        let name = path.strip_prefix(base)
            .unwrap_or(&path)
            .display()
            .to_string();

        let is_dir = path.is_dir();
        let kind = if is_dir { "dir" } else { "file" };

        let size = if is_dir {
            "—".to_string()
        } else {
            path.metadata()
                .map(|m| format_size(m.len()))
                .unwrap_or_else(|_| "—".to_string())
        };

        let modified = path.metadata()
            .and_then(|m| m.modified())
            .map(|t| {
                let secs = t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                format_timestamp(secs)
            })
            .unwrap_or_else(|_| "—".to_string());

        let display_name = if is_dir {
            format!("{}/", name)
        } else {
            name
        };

        rows.push(vec![display_name, kind.to_string(), size, modified]);

        if recursive && is_dir {
            collect_entries(base, &path, recursive, rows);
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}b", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}kb", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}mb", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn format_timestamp(secs: u64) -> String {
    // Simple YYYY-MM-DD without external deps
    let days_since_epoch = secs / 86400;
    let year = 1970 + days_since_epoch / 365;
    let day_of_year = days_since_epoch % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{:04}-{:02}-{:02}", year, month.min(12), day.min(31))
}

/// AGENT.FS.delete("path")
pub fn delete(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("delete() requires a path argument"),
    };

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    if !path.exists() {
        return AplResult::err(format!("file not found: {}", path_str));
    }

    if path.is_dir() {
        return AplResult::err(format!(
            "use deletedir() to delete directories: {}",
            path_str
        ));
    }

    match fs::remove_file(&path) {
        Ok(_) => AplResult::ok_val(format!("{} deleted", path.display())),
        Err(e) => AplResult::err(format!("delete error: {}", e)),
    }
}

/// AGENT.FS.deletedir("path")
/// Restricted to workspace/ only — cannot delete inbox, outbox, logs, .memory
pub fn deletedir(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("deletedir() requires a path argument"),
    };

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    // Only allow deletion inside workspace/
    let workspace = PathBuf::from(agent_home()).join("workspace");
    if !path.starts_with(&workspace) {
        return AplResult::err(format!(
            "protected path: deletedir only allowed inside workspace/: {}",
            path_str
        ));
    }

    if !path.exists() {
        return AplResult::err(format!("directory not found: {}", path_str));
    }

    // Count files for reporting
    let count = count_files(&path);

    match fs::remove_dir_all(&path) {
        Ok(_) => AplResult::ok_meta(vec![("removed", format!("{} files", count))])
            .with_val(format!("{} deleted", path_str)),
        Err(e) => AplResult::err(format!("deletedir error: {}", e)),
    }
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    if e.path().is_dir() {
                        count_files(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// AGENT.FS.move("src", "dst")
pub fn move_file(cmd: &Command) -> AplResult {
    let src_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("move() requires src and dst arguments"),
    };

    let dst_str = match cmd.pos_str(1) {
        Some(p) => p,
        None => return AplResult::err("move() requires a dst argument"),
    };

    let src = match safe_path(&src_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    let dst = match safe_path(&dst_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    if !src.exists() {
        return AplResult::err(format!("source not found: {}", src_str));
    }

    if let Some(parent) = dst.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return AplResult::err(format!("failed to create destination directory: {}", e));
        }
    }

    match fs::rename(&src, &dst) {
        Ok(_) => AplResult::ok_val(dst.display().to_string()),
        Err(e) => AplResult::err(format!("move error: {}", e)),
    }
}

/// AGENT.FS.exists("path")
pub fn exists(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("exists() requires a path argument"),
    };

    let path = match safe_path(&path_str) {
        Ok(p) => p,
        Err(e) => return AplResult::err(e),
    };

    let exists = path.exists();
    let kind = if path.is_dir() {
        "dir"
    } else if path.is_file() {
        "file"
    } else {
        "unknown"
    };

    if exists {
        AplResult::ok_meta(vec![
            ("exists", "true".to_string()),
            ("type", kind.to_string()),
        ])
    } else {
        AplResult::ok_meta(vec![("exists", "false".to_string())])
    }
}

/// Route an FS command to the right handler
pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "read"      => read(cmd),
        "write"     => write(cmd),
        "list"      => list(cmd),
        "delete"    => delete(cmd),
        "deletedir" => deletedir(cmd),
        "move"      => move_file(cmd),
        "exists"    => exists(cmd),
        other => AplResult::err(format!("unknown FS command: {}", other)),
    }
}
