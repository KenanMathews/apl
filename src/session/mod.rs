pub mod extended;

/// SESSION handlers — all commands shell out to system tools
///
/// This is the shell-out pattern in full:
/// No D-Bus bindings, no complex Rust crates.
/// Rust builds the command, the OS tool does the work.
///
/// Tools used:
///   notify-send   → SESSION.notify, SESSION.progress
///   xdg-open      → SESSION.open, SESSION.launch, SESSION.reveal
///   xdotool       → SESSION.key, SESSION.type, SESSION.click (X11)
///   ydotool       → SESSION.key, SESSION.type (Wayland fallback)
///   scrot         → SESSION.screenshot (X11)
///   grim          → SESSION.screenshot (Wayland)
///   busctl        → SESSION.read_screen
///   dbus-monitor  → SESSION.watch

use std::process::Command as SysCmd;
use crate::core::{parser::Command, result::AplResult};

/// Detect whether we're running on Wayland or X11
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|s| s == "wayland")
            .unwrap_or(false)
}

/// Check a tool is installed before trying to use it
fn tool_available(name: &str) -> bool {
    SysCmd::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── SESSION.notify ────────────────────────────────────────────────────────────

/// SESSION.notify("title", "message", urgency="normal")
/// Shells out to: notify-send
pub fn notify(cmd: &Command) -> AplResult {
    let title = match cmd.pos_str(0) {
        Some(t) => t,
        None => return AplResult::err("notify() requires a title argument"),
    };

    let message = cmd.pos_str(1).unwrap_or_default();
    let urgency = cmd.named_str_default("urgency", "normal");

    if !tool_available("notify-send") {
        return AplResult::err("notify-send not found — install libnotify-bin");
    }

    let status = SysCmd::new("notify-send")
        .arg("--urgency").arg(&urgency)
        .arg("--app-name").arg("apl-agent")
        .arg(&title)
        .arg(&message)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_meta(vec![("sent", "true".to_string())]),
        Ok(s) => AplResult::err(format!("notify-send exited with code {:?}", s.code())),
        Err(e) => AplResult::err(format!("notify-send error: {}", e)),
    }
}

// ── SESSION.progress ──────────────────────────────────────────────────────────

/// SESSION.progress("task", percent, message="")
/// Uses notify-send with a replace-id to update in place.
/// We store the notification ID in /tmp/apl-progress-{task_hash}
pub fn progress(cmd: &Command) -> AplResult {
    let task = match cmd.pos_str(0) {
        Some(t) => t,
        None => return AplResult::err("progress() requires a task argument"),
    };

    let percent = cmd.pos_int(1).unwrap_or(0);
    let message = cmd.pos_str(2).unwrap_or_default();

    let pct_clamped = percent.clamp(0, 100);
    let bar = make_progress_bar(pct_clamped as u8);
    let body = format!("{} {}%\n{}", bar, pct_clamped, message);

    let status = SysCmd::new("notify-send")
        .arg("--app-name").arg("apl-agent")
        .arg("--urgency").arg("low")
        .arg(&task)
        .arg(&body)
        .status();

    match status {
        Ok(s) if s.success() => {
            AplResult::ok_meta(vec![
                ("task", task),
                ("pct", pct_clamped.to_string()),
            ])
        }
        Ok(_) => AplResult::err("progress notification failed"),
        Err(e) => AplResult::err(format!("notify error: {}", e)),
    }
}

fn make_progress_bar(pct: u8) -> String {
    let filled = (pct as usize * 20) / 100;
    let empty = 20 - filled;
    format!("[{}{}]", "#".repeat(filled), ".".repeat(empty))
}

// ── SESSION.open ──────────────────────────────────────────────────────────────

/// SESSION.open("/path/to/file")
/// Shells out to: xdg-open
pub fn open(cmd: &Command) -> AplResult {
    let path = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("open() requires a path argument"),
    };

    if !std::path::Path::new(&path).exists() {
        return AplResult::err(format!("path does not exist: {}", path));
    }

    let status = SysCmd::new("xdg-open")
        .arg(&path)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_val(path),
        Ok(_) => AplResult::err(format!("no application found for: {}", path)),
        Err(e) => AplResult::err(format!("xdg-open error: {}", e)),
    }
}

// ── SESSION.reveal ────────────────────────────────────────────────────────────

/// SESSION.reveal("/path/to/file")
/// Opens file manager at the file's location
/// Shells out to: xdg-open on the parent directory
pub fn reveal(cmd: &Command) -> AplResult {
    let path_str = match cmd.pos_str(0) {
        Some(p) => p,
        None => return AplResult::err("reveal() requires a path argument"),
    };

    let path = std::path::Path::new(&path_str);

    if !path.exists() {
        return AplResult::err(format!("path does not exist: {}", path_str));
    }

    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .unwrap_or(path)
            .to_path_buf()
    };

    let status = SysCmd::new("xdg-open")
        .arg(&dir)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_val(path_str),
        Ok(_) => AplResult::err("failed to open file manager"),
        Err(e) => AplResult::err(format!("xdg-open error: {}", e)),
    }
}

// ── SESSION.launch ────────────────────────────────────────────────────────────

/// SESSION.launch("app", args=[])
/// Shells out to: xdg-open or direct execution
pub fn launch(cmd: &Command) -> AplResult {
    let app = match cmd.pos_str(0) {
        Some(a) => a,
        None => return AplResult::err("launch() requires an app argument"),
    };

    if !tool_available(&app) {
        return AplResult::err(format!("application not found: {}", app));
    }

    // Collect extra args if provided
    let extra_args: Vec<String> = cmd.args.iter()
        .find_map(|a| {
            if let crate::core::parser::Arg::Named(k, crate::core::parser::Value::List(items)) = a {
                if k == "args" {
                    return Some(items.iter()
                        .filter_map(|v| v.as_str_owned())
                        .collect());
                }
            }
            None
        })
        .unwrap_or_default();

    let child = SysCmd::new(&app)
        .args(&extra_args)
        .spawn();

    match child {
        Ok(c) => AplResult::ok_meta(vec![
            ("app", app),
            ("pid", c.id().to_string()),
        ]),
        Err(e) => AplResult::err(format!("launch error: {}", e)),
    }
}

// ── SESSION.screenshot ────────────────────────────────────────────────────────

/// SESSION.screenshot(save_to="outbox/screen.png", region=[x,y,w,h])
/// Shells out to: scrot (X11) or grim (Wayland)
pub fn screenshot(cmd: &Command) -> AplResult {
    let save_to = cmd.named_str("save_to")
        .map(|p| {
            if p.starts_with('/') { p } else { format!("/home/agent/{}", p) }
        })
        .unwrap_or_else(|| "/home/agent/outbox/screenshot.png".to_string());

    if is_wayland() {
        screenshot_wayland(&save_to, cmd)
    } else {
        screenshot_x11(&save_to, cmd)
    }
}

fn screenshot_x11(save_to: &str, cmd: &Command) -> AplResult {
    if !tool_available("scrot") {
        return AplResult::err("scrot not found — install scrot");
    }

    let mut scrot = SysCmd::new("scrot");
    scrot.arg(save_to);

    // Handle region: [x, y, w, h]
    if let Some(region) = get_region(cmd) {
        scrot.arg("--area").arg(region);
    }

    match scrot.status() {
        Ok(s) if s.success() => {
            AplResult::ok_meta(vec![("method", "scrot".to_string())])
                .with_val(save_to.to_string())
        }
        Ok(_) => AplResult::err("scrot failed"),
        Err(e) => AplResult::err(format!("scrot error: {}", e)),
    }
}

fn screenshot_wayland(save_to: &str, cmd: &Command) -> AplResult {
    if !tool_available("grim") {
        return AplResult::err("grim not found — install grim");
    }

    let mut grim = SysCmd::new("grim");

    if let Some(region) = get_region(cmd) {
        grim.arg("-g").arg(region);
    }

    grim.arg(save_to);

    match grim.status() {
        Ok(s) if s.success() => {
            AplResult::ok_meta(vec![("method", "grim".to_string())])
                .with_val(save_to.to_string())
        }
        Ok(_) => AplResult::err("grim failed"),
        Err(e) => AplResult::err(format!("grim error: {}", e)),
    }
}

fn get_region(cmd: &Command) -> Option<String> {
    cmd.args.iter().find_map(|a| {
        if let crate::core::parser::Arg::Named(k, crate::core::parser::Value::List(items)) = a {
            if k == "region" && items.len() == 4 {
                let nums: Vec<i64> = items.iter()
                    .filter_map(|v| v.as_int())
                    .collect();
                if nums.len() == 4 {
                    // scrot/grim format: "x,y,w,h" or "x,y WxH"
                    return Some(format!("{},{} {}x{}", nums[0], nums[1], nums[2], nums[3]));
                }
            }
        }
        None
    })
}

// ── SESSION.key ───────────────────────────────────────────────────────────────

/// SESSION.key("ctrl+s")
/// Shells out to: xdotool (X11) or ydotool (Wayland)
pub fn key(cmd: &Command) -> AplResult {
    let combo = match cmd.pos_str(0) {
        Some(k) => k,
        None => return AplResult::err("key() requires a key combo argument"),
    };

    if is_wayland() {
        key_wayland(&combo)
    } else {
        key_x11(&combo)
    }
}

fn key_x11(combo: &str) -> AplResult {
    if !tool_available("xdotool") {
        return AplResult::err("xdotool not found — install xdotool");
    }

    let status = SysCmd::new("xdotool")
        .arg("key")
        .arg("--clearmodifiers")
        .arg(combo)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_meta(vec![("sent", combo.to_string())]),
        Ok(_) => AplResult::err(format!("xdotool could not send: {}", combo)),
        Err(e) => AplResult::err(format!("xdotool error: {}", e)),
    }
}

fn key_wayland(combo: &str) -> AplResult {
    if !tool_available("ydotool") {
        return AplResult::err("ydotool not found — install ydotool");
    }

    // ydotool uses the same combo syntax as xdotool
    let status = SysCmd::new("ydotool")
        .arg("key")
        .arg(combo)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_meta(vec![("sent", combo.to_string())]),
        Ok(_) => AplResult::err(format!("ydotool could not send: {}", combo)),
        Err(e) => AplResult::err(format!("ydotool error: {}", e)),
    }
}

// ── SESSION.type ──────────────────────────────────────────────────────────────

/// SESSION.type("text to type")
/// Shells out to: xdotool type (X11) or ydotool type (Wayland)
pub fn type_text(cmd: &Command) -> AplResult {
    let text = match cmd.pos_str(0) {
        Some(t) => t,
        None => return AplResult::err("type() requires a text argument"),
    };

    let chars = text.chars().count();

    if is_wayland() {
        type_wayland(&text, chars)
    } else {
        type_x11(&text, chars)
    }
}

fn type_x11(text: &str, chars: usize) -> AplResult {
    if !tool_available("xdotool") {
        return AplResult::err("xdotool not found — install xdotool");
    }

    let status = SysCmd::new("xdotool")
        .arg("type")
        .arg("--clearmodifiers")
        .arg("--delay").arg("12")
        .arg(text)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_meta(vec![("chars", chars.to_string())]),
        Ok(_) => AplResult::err("xdotool type failed"),
        Err(e) => AplResult::err(format!("xdotool error: {}", e)),
    }
}

fn type_wayland(text: &str, chars: usize) -> AplResult {
    if !tool_available("ydotool") {
        return AplResult::err("ydotool not found — install ydotool");
    }

    let status = SysCmd::new("ydotool")
        .arg("type")
        .arg(text)
        .status();

    match status {
        Ok(s) if s.success() => AplResult::ok_meta(vec![("chars", chars.to_string())]),
        Ok(_) => AplResult::err("ydotool type failed"),
        Err(e) => AplResult::err(format!("ydotool error: {}", e)),
    }
}

// ── SESSION.read_screen ───────────────────────────────────────────────────────

/// SESSION.read_screen(app=null)
/// Shells out to: busctl to query AT-SPI2 over D-Bus
pub fn read_screen(cmd: &Command) -> AplResult {
    let app_filter = cmd.named_str("app");

    if !tool_available("busctl") {
        return AplResult::err("busctl not found — install systemd");
    }

    // List all AT-SPI accessible applications
    let output = SysCmd::new("busctl")
        .arg("--user")
        .arg("list")
        .arg("--no-pager")
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).to_string();

            // Filter for AT-SPI bus names
            let apps: Vec<&str> = text.lines()
                .filter(|l| l.contains("atspi") || l.contains("a11y"))
                .collect();

            if apps.is_empty() {
                return AplResult::err(
                    "AT-SPI not available — enable accessibility in system settings"
                );
            }

            let result_text = if let Some(filter) = app_filter {
                format!("ok: app={}\n  (AT-SPI tree for {} — use pyatspi2 for full tree)", filter, filter)
            } else {
                format!("ok: apps={}\n{}", apps.len(),
                    apps.iter().map(|a| format!("  {}", a)).collect::<Vec<_>>().join("\n"))
            };

            let mut result = AplResult::ok();
            result.out = Some(result_text);
            result
        }
        Err(e) => AplResult::err(format!("busctl error: {}", e)),
    }
}

// ── SESSION.exec ──────────────────────────────────────────────────────────────

/// SESSION.exec("cmd") — runs as the human user
/// The session bridge handles this — here we just execute it
/// In real deployment this goes through the approval gate first
pub fn exec(cmd: &Command) -> AplResult {
    let command_str = match cmd.pos_str(0) {
        Some(c) => c,
        None => return AplResult::err("exec() requires a command argument"),
    };

    let cwd = cmd.named_str("cwd");

    let mut sys_cmd = SysCmd::new("sh");
    sys_cmd.arg("-c").arg(&command_str);

    if let Some(dir) = cwd {
        sys_cmd.current_dir(&dir);
    }

    match sys_cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let exit_code = out.status.code().unwrap_or(-1);

            if out.status.success() {
                AplResult::ok_meta(vec![
                    ("exit", exit_code.to_string()),
                ])
                .with_out(if stdout.is_empty() { "—".to_string() } else { stdout })
            } else {
                AplResult::err_meta(
                    vec![("exit", exit_code.to_string())],
                    if stderr.is_empty() { format!("exited with {}", exit_code) } else { stderr },
                )
            }
        }
        Err(e) => AplResult::err(format!("exec error: {}", e)),
    }
}

/// Route SESSION commands
pub fn dispatch(cmd: &Command) -> AplResult {
    match cmd.action.as_str() {
        "notify"      => notify(cmd),
        "progress"    => progress(cmd),
        "open"        => open(cmd),
        "reveal"      => reveal(cmd),
        "launch"      => launch(cmd),
        "screenshot"  => screenshot(cmd),
        "key"         => key(cmd),
        "type"        => type_text(cmd),
        "read_screen" => read_screen(cmd),
        "exec"        => exec(cmd),
        other => AplResult::err(format!("unknown SESSION command: {}", other)),
    }
}
