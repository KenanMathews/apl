/// Runtime configuration — reads from environment variables set by systemd
///
/// The installer writes these into the systemd service file so the binary
/// always knows where it's operating, regardless of username chosen.

/// Get the agent home directory.
/// Reads APL_AGENT_HOME env var, falls back to /home/apl-agent
pub fn agent_home() -> String {
    std::env::var("APL_AGENT_HOME")
        .unwrap_or_else(|_| "/home/apl-agent".to_string())
}

/// Get the agent username.
/// Reads APL_AGENT_USER env var, falls back to apl-agent
pub fn agent_user() -> String {
    std::env::var("APL_AGENT_USER")
        .unwrap_or_else(|_| "apl-agent".to_string())
}

/// Get the socket path.
pub fn socket_path() -> String {
    std::env::var("APL_SOCKET")
        .unwrap_or_else(|_| "/run/apl/agent.sock".to_string())
}

/// Get the config directory.
pub fn config_dir() -> String {
    std::env::var("APL_CONFIG_DIR")
        .unwrap_or_else(|_| "/etc/apl".to_string())
}
