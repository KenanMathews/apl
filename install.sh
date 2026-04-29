#!/usr/bin/env bash
# =============================================================================
# APL — Agent Pseudocode Language
# One-click installer for Ubuntu/Debian desktop (KDE or GNOME, X11 or Wayland)
#
# Usage:
#   sudo bash install.sh          # full install
#   sudo bash install.sh --uninstall
#   sudo bash install.sh --check  # dry run — show what would be done
# =============================================================================

set -euo pipefail

# ── Constants ─────────────────────────────────────────────────────────────────

APL_VERSION="0.1.0"
GITHUB_REPO="KenanMathews/apl"
AGENT_USER="apl-agent"
AGENT_HOME="/home/apl-agent"
BINARY_PATH="/usr/local/bin/apl"
SOCKET_DIR="/run/apl"
CONFIG_DIR="/etc/apl"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colours
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# ── Helpers ───────────────────────────────────────────────────────────────────

info()    { echo -e "${BLUE}  →${NC} $*"; }
ok()      { echo -e "${GREEN}  ✔${NC} $*"; }
warn()    { echo -e "${YELLOW}  !${NC} $*"; }
err()     { echo -e "${RED}  ✘${NC} $*" >&2; }
die()     { err "$*"; exit 1; }
bold()    { echo -e "${BOLD}$*${NC}"; }
section() { echo; echo -e "${BOLD}── $* ──${NC}"; }

# Check if running as root
need_root() {
    if [[ $EUID -ne 0 ]]; then
        die "This installer must be run as root: sudo bash install.sh"
    fi
}

# Get the actual user who ran sudo (not root)
get_real_user() {
    if [[ -n "${SUDO_USER:-}" ]]; then
        echo "$SUDO_USER"
    else
        # Fallback — find first non-root user with a home dir
        awk -F: '$3 >= 1000 && $3 < 65534 {print $1; exit}' /etc/passwd
    fi
}

# Get real user's home directory
get_real_home() {
    local user
    user="$(get_real_user)"
    getent passwd "$user" | cut -d: -f6
}

# ── Checks ────────────────────────────────────────────────────────────────────

check_os() {
    if [[ ! -f /etc/os-release ]]; then
        die "Cannot detect OS — /etc/os-release not found"
    fi

    # shellcheck source=/dev/null
    source /etc/os-release

    case "$ID" in
        ubuntu|debian|linuxmint|pop)
            ok "OS: $PRETTY_NAME"
            ;;
        *)
            warn "Untested OS: $PRETTY_NAME — proceeding anyway (Debian-based expected)"
            ;;
    esac
}

check_desktop() {
    local session="${XDG_SESSION_TYPE:-unknown}"
    local desktop="${XDG_CURRENT_DESKTOP:-unknown}"

    # If running under sudo, check the real user's session
    local real_user
    real_user="$(get_real_user)"

    ok "Desktop: ${desktop} (${session})"

    if [[ "$session" == "wayland" ]]; then
        warn "Wayland session detected — SESSION.key and SESSION.type will use ydotool"
        warn "SESSION.screenshot will use grim instead of scrot"
    fi
}

check_binary() {
    # 1. Already built locally (dev workflow)
    local local_bin="${SCRIPT_DIR}/target/release/apl"
    [[ -f "$local_bin" ]] && { echo "$local_bin"; return; }
    local_bin="./apl"
    [[ -f "$local_bin" ]] && { echo "$local_bin"; return; }

    # 2. Download from GitHub Releases
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64)  arch_suffix="x86_64-linux" ;;
        aarch64) arch_suffix="aarch64-linux" ;;
        *) die "Unsupported architecture: $arch" ;;
    esac

    local url="https://github.com/${GITHUB_REPO}/releases/download/v${APL_VERSION}/apl-${arch_suffix}"
    local tmp
    tmp="$(mktemp)"

    info "Downloading APL v${APL_VERSION} binary for ${arch}..."
    if curl -fsSL --retry 3 --retry-delay 2 -o "$tmp" "$url"; then
        chmod +x "$tmp"
        echo "$tmp"
    else
        rm -f "$tmp"
        die "Download failed from: $url\nBuild locally with: cargo build --release"
    fi
}

# ── Dependency installation ───────────────────────────────────────────────────

install_deps() {
    section "Installing system dependencies"

    # Core tools always needed
    local always_needed=(
        "notify-send:libnotify-bin"
        "xdg-open:xdg-utils"
        "curl:curl"
        "busctl:systemd"
        "dbus-monitor:dbus"
        "inotifywait:inotify-tools"
    )

    # X11 tools
    local x11_tools=(
        "xdotool:xdotool"
        "scrot:scrot"
        "wmctrl:wmctrl"
    )

    # Wayland tools
    local wayland_tools=(
        "ydotool:ydotool"
        "grim:grim"
    )

    # Dialog tools
    local dialog_tools=(
        "zenity:zenity"
    )

    local to_install=()

    # Check what's missing
    check_and_queue() {
        local tool="${1%%:*}"
        local pkg="${1##*:}"
        if ! command -v "$tool" &>/dev/null; then
            to_install+=("$pkg")
            info "Will install: $pkg (provides $tool)"
        else
            ok "Already installed: $tool"
        fi
    }

    for dep in "${always_needed[@]}";  do check_and_queue "$dep"; done
    for dep in "${x11_tools[@]}";      do check_and_queue "$dep"; done
    for dep in "${wayland_tools[@]}";  do check_and_queue "$dep"; done
    for dep in "${dialog_tools[@]}";   do check_and_queue "$dep"; done

    if [[ ${#to_install[@]} -gt 0 ]]; then
        info "Running apt install..."
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y "${to_install[@]}" \
            --no-install-recommends -qq
        ok "Dependencies installed"
    else
        ok "All dependencies already present"
    fi
}

# ── Binary installation ───────────────────────────────────────────────────────

install_binary() {
    section "Installing APL binary"

    local src
    src="$(check_binary)"

    install -m 755 "$src" "$BINARY_PATH"
    [[ "$src" == /tmp/* ]] && rm -f "$src"
    ok "Binary installed: $BINARY_PATH"

    # Verify it runs
    if "$BINARY_PATH" 'AGENT.SYS.capabilities()' &>/dev/null; then
        ok "Binary self-test passed"
    else
        warn "Binary self-test failed — check permissions"
    fi
}

# ── Agent user ────────────────────────────────────────────────────────────────

create_agent_user() {
    section "Creating agent user"

    if id "$AGENT_USER" &>/dev/null; then
        ok "User already exists: $AGENT_USER"
    else
        useradd \
            --system \
            --no-create-home \
            --shell /usr/sbin/nologin \
            --comment "APL Agent User" \
            --home-dir "$AGENT_HOME" \
            "$AGENT_USER"
        ok "User created: $AGENT_USER"
    fi

    # Create home directory structure
    local dirs=(
        "$AGENT_HOME"
        "$AGENT_HOME/workspace"
        "$AGENT_HOME/inbox"
        "$AGENT_HOME/outbox"
        "$AGENT_HOME/logs"
        "$AGENT_HOME/.memory"
    )

    for dir in "${dirs[@]}"; do
        if [[ ! -d "$dir" ]]; then
            mkdir -p "$dir"
            info "Created: $dir"
        fi
    done

    # Set ownership and permissions
    chown -R "$AGENT_USER:$AGENT_USER" "$AGENT_HOME"
    chmod 750 "$AGENT_HOME"
    chmod 755 "$AGENT_HOME/workspace"
    chmod 755 "$AGENT_HOME/inbox"
    chmod 755 "$AGENT_HOME/outbox"  # human user needs read access
    chmod 700 "$AGENT_HOME/logs"
    chmod 700 "$AGENT_HOME/.memory"

    # Allow the real human user to read outbox (agent writes, human reads)
    local real_user
    real_user="$(get_real_user)"
    if command -v setfacl &>/dev/null; then
        setfacl -m "u:${real_user}:rx" "$AGENT_HOME/outbox" 2>/dev/null || true
    fi

    ok "Agent home configured: $AGENT_HOME"
}

# ── Config ────────────────────────────────────────────────────────────────────

install_config() {
    section "Installing configuration"

    mkdir -p "$CONFIG_DIR"

    local real_user
    real_user="$(get_real_user)"

    # Only write default config if none exists
    if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
        cat > "$CONFIG_DIR/config.toml" << TOML
# APL Configuration
# Edit this file to change APL behaviour.
# Restart apl-agent after making changes.

[agent]
user = "${AGENT_USER}"
home = "${AGENT_HOME}"
socket = "/run/apl/agent.sock"

[agent.limits]
# Maximum command timeout in seconds
max_timeout = 300
# Maximum file size the agent can read at once (bytes)
max_read_bytes = 10485760  # 10MB
# Maximum outbox size before warnings (bytes)
max_outbox_bytes = 104857600  # 100MB

[session]
# Human user who owns the desktop session
user = "${real_user}"
# Socket for session bridge
socket = "/run/user/\$(id -u ${real_user})/apl-session.sock"

[session.approval]
# Commands that always require approval (never whitelist)
always_ask = ["exec", "type"]
# Commands that require first-time approval then are whitelisted
whitelist_once = ["launch", "screenshot", "read_screen", "key", "click"]
# Commands that never need approval
never_ask = ["notify", "progress", "open", "reveal"]

[providers]
# Configure LLM providers here
# Example:
# [providers.ollama]
# url = "http://localhost:11434"
# default_model = "llama3.1:8b"
#
# [providers.anthropic]
# api_key_env = "ANTHROPIC_API_KEY"
# default_model = "claude-sonnet-4-6"
TOML
        ok "Config written: $CONFIG_DIR/config.toml"
    else
        ok "Config already exists — skipping: $CONFIG_DIR/config.toml"
    fi

    chmod 644 "$CONFIG_DIR/config.toml"
}

# ── Polkit policy ─────────────────────────────────────────────────────────────

install_polkit() {
    section "Installing polkit policy"

    local policy_dir="/usr/share/polkit-1/actions"
    local policy_file="$policy_dir/com.apl.policy"

    if [[ ! -d "$policy_dir" ]]; then
        warn "polkit not found — skipping policy installation"
        return
    fi

    cat > "$policy_file" << 'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
    "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
    "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>

  <vendor>APL Agent System</vendor>
  <vendor_url>https://github.com/apl-agent</vendor_url>

  <!--
    Action: apl-agent can request to run a command as the session user.
    This is used for SESSION.exec — the human must approve via polkit dialog.
  -->
  <action id="com.apl.session.exec">
    <description>Run a command as the desktop user</description>
    <message>The APL agent is requesting to run a command as you. Review the command carefully before approving.</message>
    <defaults>
      <allow_any>auth_self</allow_any>
      <allow_inactive>auth_self</allow_inactive>
      <allow_active>auth_self</allow_active>
    </defaults>
  </action>

  <!--
    Action: apl-agent can inject keyboard input via ydotool/xdotool.
    Requires one-time approval per session.
  -->
  <action id="com.apl.session.input">
    <description>Send keyboard or mouse input to the desktop</description>
    <message>The APL agent is requesting to send keyboard or mouse input to your desktop session.</message>
    <defaults>
      <allow_any>auth_self</allow_any>
      <allow_inactive>auth_self</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
  </action>

  <!--
    Action: read the screen accessibility tree.
    Lower risk — only reads, does not write.
  -->
  <action id="com.apl.session.read_screen">
    <description>Read the desktop accessibility tree</description>
    <message>The APL agent is requesting to read the UI element tree of your desktop.</message>
    <defaults>
      <allow_any>auth_self</allow_any>
      <allow_inactive>auth_self</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
  </action>

</policyconfig>
XML

    chmod 644 "$policy_file"
    ok "Polkit policy installed: $policy_file"
}

# ── D-Bus config ──────────────────────────────────────────────────────────────

install_dbus() {
    section "Installing D-Bus configuration"

    local dbus_dir="/usr/share/dbus-1/system.d"
    local dbus_file="$dbus_dir/com.apl.conf"

    if [[ ! -d "$dbus_dir" ]]; then
        warn "D-Bus system.d not found — skipping"
        return
    fi

    cat > "$dbus_file" << XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE busconfig PUBLIC
    "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
    "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>

  <!-- Only the apl-agent user can register this service -->
  <policy user="${AGENT_USER}">
    <allow own="com.apl.Agent"/>
  </policy>

  <!-- Any user on the system can send messages to the agent -->
  <policy context="default">
    <allow send_destination="com.apl.Agent"/>
    <allow receive_sender="com.apl.Agent"/>
  </policy>

</busconfig>
XML

    chmod 644 "$dbus_file"
    ok "D-Bus config installed: $dbus_file"
}

# ── Systemd services ──────────────────────────────────────────────────────────

install_system_service() {
    section "Installing system daemon (apl-agent)"

    # Create runtime directory config
    cat > "/etc/tmpfiles.d/apl.conf" << EOF
# APL runtime directory
d /run/apl 0750 ${AGENT_USER} ${AGENT_USER} -
EOF

    # Apply immediately
    systemd-tmpfiles --create /etc/tmpfiles.d/apl.conf 2>/dev/null || \
        mkdir -p /run/apl && chown "$AGENT_USER:$AGENT_USER" /run/apl && chmod 750 /run/apl

    cat > "/etc/systemd/system/apl-agent.service" << EOF
[Unit]
Description=APL Agent Interpreter Daemon
Documentation=https://github.com/apl-agent
After=network.target dbus.service
Wants=dbus.service

[Service]
Type=simple
User=${AGENT_USER}
Group=${AGENT_USER}
Environment=APL_AGENT_HOME=${AGENT_HOME}
Environment=APL_AGENT_USER=${AGENT_USER}
Environment=APL_SOCKET=/run/apl/agent.sock
Environment=HOME=${AGENT_HOME}
ExecStart=${BINARY_PATH} --daemon /run/apl/agent.sock
ExecReload=/bin/kill -HUP \$MAINPID
Restart=on-failure
RestartSec=5s
RuntimeDirectory=apl
RuntimeDirectoryMode=0750

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${AGENT_HOME} /run/apl
CapabilityBoundingSet=
AmbientCapabilities=

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=apl-agent

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable apl-agent.service
    systemctl start apl-agent.service

    if systemctl is-active --quiet apl-agent.service; then
        ok "apl-agent service running"
    else
        warn "apl-agent service failed to start — check: journalctl -u apl-agent"
    fi
}

install_user_service() {
    section "Installing user session daemon (apl-session)"

    local real_user
    real_user="$(get_real_user)"
    local real_home
    real_home="$(get_real_home)"
    local user_systemd_dir="$real_home/.config/systemd/user"

    mkdir -p "$user_systemd_dir"

    cat > "$user_systemd_dir/apl-session.service" << EOF
[Unit]
Description=APL Session Bridge
Documentation=https://github.com/apl-agent
After=graphical-session.target dbus.socket
Requires=dbus.socket

[Service]
Type=simple
ExecStart=${BINARY_PATH} --session-bridge
Restart=on-failure
RestartSec=3s

# Pass through display environment so we can reach the desktop
PassEnvironment=DISPLAY WAYLAND_DISPLAY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR XDG_SESSION_TYPE

StandardOutput=journal
StandardError=journal
SyslogIdentifier=apl-session

[Install]
WantedBy=graphical-session.target
EOF

    chown -R "$real_user:$real_user" "$user_systemd_dir"

    # Enable as the real user
    sudo -u "$real_user" systemctl --user daemon-reload 2>/dev/null || true
    sudo -u "$real_user" systemctl --user enable apl-session.service 2>/dev/null || true

    ok "Session service installed for user: $real_user"
    warn "Session service will start on next login (requires graphical session)"
}

# ── Autostart ─────────────────────────────────────────────────────────────────

install_autostart() {
    section "Installing manager UI autostart"

    local autostart_dir="/etc/xdg/autostart"
    mkdir -p "$autostart_dir"

    cat > "$autostart_dir/apl-manager.desktop" << EOF
[Desktop Entry]
Type=Application
Name=APL Agent Manager
Comment=APL agent system tray manager
Exec=${BINARY_PATH} --manager
Icon=utilities-terminal
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
X-KDE-autostart-after=panel
StartupNotify=false
EOF

    chmod 644 "$autostart_dir/apl-manager.desktop"
    ok "Autostart entry installed"
}

# ── AT-SPI2 enablement ────────────────────────────────────────────────────────

enable_atspi() {
    section "Enabling AT-SPI2 accessibility"

    local env_file="/etc/environment"

    if grep -q "NO_AT_BRIDGE" "$env_file" 2>/dev/null; then
        ok "AT-SPI2 already configured"
        return
    fi

    # Only add if not already there
    if ! grep -q "AT_SPI_BUS_ADDRESS" "$env_file" 2>/dev/null; then
        echo "" >> "$env_file"
        echo "# APL Agent System — AT-SPI2 accessibility" >> "$env_file"
        echo "NO_AT_BRIDGE=0" >> "$env_file"
        ok "AT-SPI2 enabled in /etc/environment"
    fi
}

# ── ydotool setup ─────────────────────────────────────────────────────────────

setup_ydotool() {
    section "Configuring ydotool (Wayland input injection)"

    if ! command -v ydotool &>/dev/null; then
        warn "ydotool not installed — Wayland input injection unavailable"
        return
    fi

    # ydotool needs its daemon running and the agent user needs access to /dev/uinput
    local uinput_rule="/etc/udev/rules.d/80-apl-uinput.rules"

    cat > "$uinput_rule" << EOF
# APL Agent — allow ydotool access to uinput for input injection
KERNEL=="uinput", GROUP="input", MODE="0660"
EOF

    # Add agent user and real user to input group
    usermod -aG input "$AGENT_USER" 2>/dev/null || true
    local real_user
    real_user="$(get_real_user)"
    usermod -aG input "$real_user" 2>/dev/null || true

    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true

    # Install ydotoold as a system service
    cat > "/etc/systemd/system/ydotoold.service" << 'EOF'
[Unit]
Description=ydotool daemon
After=systemd-udev-settle.service

[Service]
Type=simple
ExecStart=/usr/bin/ydotoold
Restart=on-failure
RuntimeDirectory=ydotool
RuntimeDirectoryMode=0770

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable ydotoold.service 2>/dev/null || true
    systemctl start ydotoold.service 2>/dev/null || true

    ok "ydotool configured"
}

# ── Verification ──────────────────────────────────────────────────────────────

verify_install() {
    section "Verifying installation"

    local all_ok=true

    check_item() {
        local label="$1"
        local cmd="$2"
        if eval "$cmd" &>/dev/null; then
            ok "$label"
        else
            warn "NOT OK: $label"
            all_ok=false
        fi
    }

    check_item "Binary installed"           "test -x $BINARY_PATH"
    check_item "Agent user exists"          "id $AGENT_USER"
    check_item "Agent home exists"          "test -d $AGENT_HOME"
    check_item "Workspace dir exists"       "test -d $AGENT_HOME/workspace"
    check_item "Inbox dir exists"           "test -d $AGENT_HOME/inbox"
    check_item "Outbox dir exists"          "test -d $AGENT_HOME/outbox"
    check_item "Config exists"              "test -f $CONFIG_DIR/config.toml"
    check_item "System service enabled"     "systemctl is-enabled --quiet apl-agent"
    check_item "System service active"      "systemctl is-active --quiet apl-agent"
    check_item "notify-send available"      "command -v notify-send"
    check_item "xdg-open available"         "command -v xdg-open"

    echo
    if $all_ok; then
        ok "All checks passed"
    else
        warn "Some checks failed — see above"
    fi
}

# ── Uninstall ─────────────────────────────────────────────────────────────────

uninstall() {
    section "Uninstalling APL"

    warn "This will remove all APL components."
    warn "Agent home directory (${AGENT_HOME}) will be preserved."
    read -rp "  Continue? [y/N] " confirm
    [[ "$confirm" =~ ^[Yy]$ ]] || { info "Cancelled."; exit 0; }

    # Stop and disable services
    systemctl stop apl-agent.service 2>/dev/null || true
    systemctl disable apl-agent.service 2>/dev/null || true

    local real_user
    real_user="$(get_real_user)"
    sudo -u "$real_user" systemctl --user stop apl-session.service 2>/dev/null || true
    sudo -u "$real_user" systemctl --user disable apl-session.service 2>/dev/null || true

    # Remove files
    local files=(
        "$BINARY_PATH"
        "/etc/systemd/system/apl-agent.service"
        "/etc/tmpfiles.d/apl.conf"
        "/usr/share/polkit-1/actions/com.apl.policy"
        "/usr/share/dbus-1/system.d/com.apl.conf"
        "/etc/xdg/autostart/apl-manager.desktop"
        "/etc/udev/rules.d/80-apl-uinput.rules"
        "/etc/systemd/system/ydotoold.service"
        "$(get_real_home)/.config/systemd/user/apl-session.service"
    )

    for f in "${files[@]}"; do
        if [[ -f "$f" ]]; then
            rm -f "$f"
            info "Removed: $f"
        fi
    done

    systemctl daemon-reload

    ok "APL uninstalled"
    warn "Config preserved at: $CONFIG_DIR"
    warn "Agent home preserved at: $AGENT_HOME"
    info "Remove manually if desired:"
    info "  sudo rm -rf $CONFIG_DIR $AGENT_HOME"
    info "  sudo userdel $AGENT_USER"
}

# ── Dry run ───────────────────────────────────────────────────────────────────

dry_run() {
    bold "APL Installer — Dry Run (no changes will be made)"
    echo

    local real_user
    real_user="$(get_real_user)"

    echo "Would create user:      $AGENT_USER (no shell, no login)"
    echo "Would create home:      $AGENT_HOME/{workspace,inbox,outbox,logs,.memory}"
    echo "Would install binary:   $BINARY_PATH"
    echo "Would install config:   $CONFIG_DIR/config.toml"
    echo "Would install service:  /etc/systemd/system/apl-agent.service"
    echo "Would install service:  $(get_real_home)/.config/systemd/user/apl-session.service"
    echo "Would install policy:   /usr/share/polkit-1/actions/com.apl.policy"
    echo "Would install dbus:     /usr/share/dbus-1/system.d/com.apl.conf"
    echo "Would install autostart:/etc/xdg/autostart/apl-manager.desktop"
    echo "Would enable AT-SPI2 in /etc/environment"
    echo "Would configure ydotool for Wayland input"
    echo
    echo "Human user:             $real_user"
    echo "Human home:             $(get_real_home)"
    echo
    echo "Run without --check to perform actual installation."
}

# ── Main ──────────────────────────────────────────────────────────────────────

main() {
    echo
    bold "APL — Agent Pseudocode Language v${APL_VERSION}"
    bold "One-click installer"
    echo

    case "${1:-}" in
        --uninstall)
            need_root
            uninstall
            exit 0
            ;;
        --check)
            dry_run
            exit 0
            ;;
        --help|-h)
            echo "Usage: sudo bash install.sh [--uninstall | --check | --help]"
            echo
            echo "  (no args)    Full installation"
            echo "  --uninstall  Remove APL from this system"
            echo "  --check      Show what would be installed (dry run)"
            exit 0
            ;;
        "")
            need_root
            ;;
        *)
            die "Unknown argument: $1 — use --help"
            ;;
    esac

    # Full install
    check_os
    check_desktop
    install_deps
    install_binary
    create_agent_user
    install_config
    install_polkit
    install_dbus
    install_system_service
    install_user_service
    install_autostart
    enable_atspi
    setup_ydotool
    verify_install

    echo
    bold "Installation complete!"
    echo
    echo "  Agent user:    $AGENT_USER"
    echo "  Agent home:    $AGENT_HOME"
    echo "  Binary:        $BINARY_PATH"
    echo "  Config:        $CONFIG_DIR/config.toml"
    echo "  System socket: /run/apl/agent.sock"
    echo
    echo "  Quick test:"
    echo "    apl 'AGENT.SYS.capabilities()'"
    echo "    apl 'AGENT.FS.list(\"workspace\")'"
    echo
    warn "Log out and back in for the session bridge and AT-SPI2 to activate."
    echo
}

main "$@"
