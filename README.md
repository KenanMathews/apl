# APL — Agent Pseudocode Language

A compact, structured command language that lets an LLM agent interact with a Linux desktop OS. Built in Rust. Ships as a single binary with no runtime dependencies.

---

## What it is

APL gives an LLM agent a well-defined interface to a Linux system. Instead of generating raw bash or verbose natural language, the agent writes structured pseudocode commands that map directly to OS operations.

```
AGENT.FS.write("workspace/report.txt", "Analysis complete.")
AGENT.SYS.run("python3 analyse.py")
AGENT.MEM.save("task_context", "Processing CSV from inbox.")
SESSION.notify("Done", "Report is ready in your outbox.")
```

Every command returns a consistent tagged-line format — token-efficient, grep-friendly, and unambiguous.

```
ok: exit=0 time=842ms
out: 3 rows processed
err: —

er: exit=1 time=210ms
out: —
err: ModuleNotFoundError: No module named 'pandas'
```

---

## Two modes — completely separate

**AGENT mode** — the agent runs as its own sandboxed Linux user (`apl-agent`). It works independently in the background, like a colleague on the same machine. It cannot touch your files, your desktop, or anything outside its own home directory.

**SESSION mode** — the agent acts on your desktop session, as you. Opens apps, sends notifications, reads the screen, injects keyboard input. Requires approval for anything that touches your environment.

---

## Quick start

### Requirements

- Linux (Ubuntu 24.04 recommended, Debian-based)
- KDE Plasma or GNOME desktop (X11 session recommended for full SESSION support)
- Rust 1.75+ (for building from source)
- `curl`, `git`

### Install from source

```bash
git clone https://github.com/yourname/apl
cd apl
cargo build --release
sudo bash install.sh
```

### One-liner (once a release binary is published)

```bash
curl -fsSL https://raw.githubusercontent.com/yourname/apl/main/install.sh | sudo bash
```

### Verify install

```bash
apl 'AGENT.SYS.capabilities()'
apl 'AGENT.FS.write("workspace/hello.txt", "hello world")'
apl 'AGENT.FS.read("workspace/hello.txt")'
```

---

## Usage

APL has three run modes:

```bash
# REPL — interactive, reads from stdin
apl

# Single command
apl 'AGENT.FS.list("workspace")'

# Daemon — Unix socket server for persistent agent sessions
apl --daemon /run/apl/agent.sock
```

---

## Command reference

### AGENT mode

Runs as the `apl-agent` system user. All paths sandboxed to `/home/apl-agent/`.

#### AGENT.FS — Filesystem

```
AGENT.FS.read("workspace/notes.txt")
AGENT.FS.write("outbox/result.txt", "content", append=false)
AGENT.FS.list("workspace/", recursive=false)
AGENT.FS.exists("workspace/config.json")
AGENT.FS.move("workspace/draft.txt", "outbox/final.txt")
AGENT.FS.delete("workspace/temp.txt")
AGENT.FS.deletedir("workspace/old_project/")
```

#### AGENT.MEM — Persistent memory

Survives restarts. Named text blobs stored in `/home/apl-agent/.memory/`.

```
AGENT.MEM.save("project_context", "Working on CSV analysis.")
AGENT.MEM.recall("project_context")
AGENT.MEM.list()
AGENT.MEM.delete("old_context")
```

#### AGENT.SYS — System execution

```
AGENT.SYS.run("python3 analyse.py", cwd="workspace", timeout=30)
AGENT.SYS.capabilities()
AGENT.SYS.env("HOME")
```

#### AGENT.PROC — Process management

```
AGENT.PROC.list()
AGENT.PROC.kill(4821)
```

#### AGENT.NET — Network

Outbound HTTP only. No listening on ports.

```
AGENT.NET.fetch("https://api.example.com/data")
AGENT.NET.fetch("https://api.example.com/submit", method="POST", body="{\"key\":\"val\"}")
```

#### AGENT.LOG — Logging

```
AGENT.LOG.write("info", "Started analysis task")
AGENT.LOG.write("warn", "File was empty, skipping")
AGENT.LOG.write("error", "API connection failed")
AGENT.LOG.read(lines=50)
AGENT.LOG.clear()
```

#### AGENT.ESCALATE — Hand back to human

Stops the agent and shows a dialog asking you to make a decision. Agent waits for your response.

```
AGENT.ESCALATE("I need to write outside my workspace. Confirm or suggest alternative.")
AGENT.ESCALATE("API returned 403.", options=["provide credentials", "skip", "cancel"])
```

---

### SESSION mode

Runs as you, in your desktop session. Has access to your display, D-Bus, and AT-SPI element tree.

#### Approval tiers

| Tier | Commands | Approval |
|---|---|---|
| Always allowed | `notify` `progress` `open` `reveal` | Never |
| Whitelist | `launch` `screenshot` `read_screen` `key` `click` | First time only |
| Always ask | `exec` `type` | Every time |

#### Commands

```
SESSION.notify("Title", "Message", urgency="normal")
SESSION.progress("Task name", 45, "Processing rows...")
SESSION.ask("Which folder?", options=["Documents", "Downloads"], default="Documents")
SESSION.open("/home/apl-agent/outbox/report.pdf")
SESSION.reveal("/home/apl-agent/outbox/report.pdf")
SESSION.launch("firefox", args=["https://example.com"])
SESSION.screenshot(save_to="outbox/screen.png")
SESSION.read_screen(app="gedit")
SESSION.key("ctrl+s")
SESSION.type("Hello from the agent.")
SESSION.click(element_text="Save")
SESSION.exec("cp /home/apl-agent/outbox/report.pdf /home/alice/Documents/")
SESSION.watch("window")
SESSION.watch("file", filter="/home/alice/Downloads")
SESSION.watch("idle")
```

---

## Return format

Every command returns tagged lines. No JSON, no brackets.

```
# Success
ok: exit=0 time=842ms
out: hello world
err: —

# Failure
er: exit=1 time=210ms
out: —
err: command not found: python4

# List
ok: count=3
  notes.txt   file  1.0kb  2026-04-29
  data/        dir    —    2026-04-29
  script.py   file  4.2kb  2026-04-28

# Single value
ok: key=project_context
val: Working on CSV analysis.

# Stream (SESSION.watch)
ok: watching=window
ev: window_changed  app=firefox  title=GitHub
ev: window_changed  app=gedit    title=notes.txt
```

---

## Project structure

```
apl/
├── Cargo.toml
├── install.sh               ← one-click installer
└── src/
    ├── main.rs              ← entry point, dispatcher, three run modes
    ├── core/
    │   ├── config.rs        ← runtime config (reads APL_AGENT_HOME env var)
    │   ├── parser.rs        ← APL command parser
    │   ├── result.rs        ← tagged-line return format
    │   └── server.rs        ← Unix domain socket daemon
    ├── agent/
    │   ├── fs.rs            ← AGENT.FS.* — filesystem operations
    │   ├── sys.rs           ← AGENT.SYS.* — execution and system info
    │   ├── mem.rs           ← AGENT.MEM.* — persistent key-value memory
    │   ├── net.rs           ← AGENT.NET.* — outbound HTTP via curl
    │   ├── proc.rs          ← AGENT.PROC.* — process management
    │   ├── log.rs           ← AGENT.LOG.* — structured logging
    │   └── escalate.rs      ← AGENT.ESCALATE — hand back to human
    └── session/
        ├── mod.rs           ← SESSION.* — desktop commands via system tools
        └── extended.rs      ← SESSION.ask, SESSION.watch
```

---

## How SESSION mode works under the hood

No D-Bus bindings in Rust. Every SESSION command shells out to a system tool that is already on the machine:

| APL command | Tool used |
|---|---|
| `SESSION.notify` | `notify-send` |
| `SESSION.open` / `SESSION.reveal` | `xdg-open` |
| `SESSION.launch` | `xdg-open` / direct exec |
| `SESSION.screenshot` | `scrot` (X11) / `grim` (Wayland) |
| `SESSION.key` / `SESSION.type` | `xdotool` (X11) / `ydotool` (Wayland) |
| `SESSION.click` | `xdotool` |
| `SESSION.read_screen` | `busctl` → AT-SPI2 |
| `SESSION.watch` | `dbus-monitor` + `inotifywait` |
| `SESSION.ask` | `zenity` / `kdialog` |
| `SESSION.exec` | `sh -c` as human user |

X11 vs Wayland is detected automatically at runtime via `$WAYLAND_DISPLAY` and `$XDG_SESSION_TYPE`.

---

## Agent home directory

```
/home/apl-agent/
├── workspace/      ← agent works here
├── inbox/          ← tasks and input files come in here
├── outbox/         ← results go here for the human to consume
├── logs/
│   └── apl.log
└── .memory/        ← persistent key-value memory blobs
```

---

## Security model

The agent user (`apl-agent`) is a system account with no shell and no login. It cannot escalate privileges, access other users' files, or listen on network ports. All filesystem operations are sandboxed to `/home/apl-agent/` — path traversal attempts are rejected before the OS sees them.

SESSION commands that act on your desktop session require approval via polkit dialog. The approval tier for each command is fixed and cannot be overridden by the agent.

| Blocked always |
|---|
| Access to files outside `/home/apl-agent/` |
| Sudo or root actions |
| Listening on network ports |
| Access to other users' files |

---

## Configuration

Installed at `/etc/apl/config.toml`. Edit and restart `apl-agent` to apply changes.

```toml
[agent]
user = "apl-agent"
home = "/home/apl-agent"
socket = "/run/apl/agent.sock"

[agent.limits]
max_timeout = 300
max_read_bytes = 10485760

[session]
user = "alice"

[session.approval]
always_ask = ["exec", "type"]
whitelist_once = ["launch", "screenshot", "read_screen", "key", "click"]
never_ask = ["notify", "progress", "open", "reveal"]
```

---

## Environment variables

The binary reads these at startup, set automatically by the systemd service:

| Variable | Default | Purpose |
|---|---|---|
| `APL_AGENT_HOME` | `/home/apl-agent` | Agent home directory |
| `APL_AGENT_USER` | `apl-agent` | Agent username |
| `APL_SOCKET` | `/run/apl/agent.sock` | Unix socket path |
| `APL_CONFIG_DIR` | `/etc/apl` | Config directory |

---

## Building

```bash
# Debug build
cargo build

# Release build (optimised, ~13MB single binary)
cargo build --release

# Run tests
cargo test

# Run tests with non-default agent home
APL_AGENT_HOME=/home/apl-agent cargo test
```

---

## Publishing a release

```bash
# Tag the release
git tag v0.1.0
git push origin v0.1.0

# Build release binary
cargo build --release

# Upload to GitHub Releases (requires gh CLI)
gh release create v0.1.0 \
  target/release/apl \
  --title "APL v0.1.0" \
  --notes "Initial release"
```

---

## Testing in WSL2

APL works in WSL2 with WSLg. AGENT mode works immediately with no desktop. SESSION mode requires a running X11 session.

```bash
# Enable systemd in WSL first
echo -e "[boot]\nsystemd=true" | sudo tee /etc/wsl.conf

# Shut down WSL from PowerShell, then reopen
# wsl --shutdown

# Install and test
cargo build --release
sudo bash install.sh
apl 'AGENT.SYS.capabilities()'
```

For SESSION mode in WSL, start a desktop first:

```bash
sudo apt install -y xfce4
unset WAYLAND_DISPLAY
export DISPLAY=:0
export GDK_BACKEND=x11
eval $(dbus-launch --sh-syntax)
startxfce4 &
```

---

## Status

| Component | Status |
|---|---|
| APL parser | Complete |
| Return format | Complete |
| AGENT.FS | Complete |
| AGENT.SYS | Complete |
| AGENT.MEM | Complete |
| AGENT.NET | Complete |
| AGENT.PROC | Complete |
| AGENT.LOG | Complete |
| AGENT.ESCALATE | Complete |
| SESSION.notify / progress | Complete |
| SESSION.open / reveal / launch | Complete |
| SESSION.screenshot | Complete |
| SESSION.key / type / click | Complete |
| SESSION.read_screen | Complete |
| SESSION.ask | Complete |
| SESSION.watch | Complete |
| SESSION.exec | Complete |
| Unix socket daemon | Complete |
| One-click installer | Complete |
| Agent manager UI | Planned |
| Config file parsing | Planned |
| Provider integration (Ollama, Anthropic) | Planned |

---

## Spec

The full APL language specification is in `APL_SPEC_v0.2.md`. It documents every command, its return format, its underlying mechanism, and its approval tier.

---

## License

MIT