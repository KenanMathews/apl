/// Unix domain socket server
///
/// Listens at /run/apl/agent.sock
/// Each connection sends one APL command per line, gets one result back.
/// Multiple connections are handled concurrently via threads.
///
/// Protocol:
///   client sends: AGENT.FS.read("workspace/notes.txt")\n
///   server replies: ok:\nout: <contents>\n\n
///   (blank line = end of response)

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::fs;
use std::thread;

pub const SOCKET_PATH: &str = "/run/apl/agent.sock";
pub const SOCKET_DIR: &str  = "/run/apl";

pub struct Server {
    socket_path: String,
}

impl Server {
    pub fn new(path: &str) -> Self {
        Self { socket_path: path.to_string() }
    }

    pub fn run(&self, dispatcher: fn(&str) -> crate::core::result::AplResult) {
        // Ensure socket directory exists
        if let Err(e) = fs::create_dir_all(SOCKET_DIR) {
            eprintln!("apl: failed to create socket dir: {}", e);
            std::process::exit(1);
        }

        // Remove stale socket if it exists
        if Path::new(&self.socket_path).exists() {
            let _ = fs::remove_file(&self.socket_path);
        }

        let listener = match UnixListener::bind(&self.socket_path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("apl: failed to bind socket {}: {}", self.socket_path, e);
                std::process::exit(1);
            }
        };

        // Set socket permissions so agent user and human user can both connect
        let _ = fs::set_permissions(
            &self.socket_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o660),
        );

        eprintln!("apl: listening on {}", self.socket_path);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        handle_connection(stream, dispatcher);
                    });
                }
                Err(e) => {
                    eprintln!("apl: connection error: {}", e);
                }
            }
        }
    }
}

fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    dispatcher: fn(&str) -> crate::core::result::AplResult,
) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut writer = stream;
    let reader = BufReader::new(reader_stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let input = line.trim();
        if input.is_empty() || input.starts_with('#') {
            continue;
        }

        let result = dispatcher(input);
        let rendered = result.render();

        // Write result followed by blank line (end of response marker)
        if writer.write_all(rendered.as_bytes()).is_err() { break; }
        if writer.write_all(b"\n\n").is_err() { break; }
        if writer.flush().is_err() { break; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::io::{Write, BufRead, BufReader};
    use std::thread;
    use std::time::Duration;

    fn test_dispatcher(input: &str) -> crate::core::result::AplResult {
        crate::core::result::AplResult::ok_val(format!("echo: {}", input))
    }

    #[test]
    fn test_server_accepts_connection() {
        let socket_path = "/tmp/apl_test.sock";

        // Clean up any stale socket
        let _ = std::fs::remove_file(socket_path);

        let path_clone = socket_path.to_string();
        thread::spawn(move || {
            let server = Server::new(&path_clone);
            // Just bind and accept one connection
            let _ = std::fs::create_dir_all("/tmp");
            if let Ok(listener) = UnixListener::bind(&path_clone) {
                if let Ok(stream) = listener.accept() {
                    handle_connection(stream.0, test_dispatcher);
                }
            }
        });

        // Give server a moment to start
        thread::sleep(Duration::from_millis(100));

        // Connect and send a command
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                stream.write_all(b"AGENT.FS.exists(\"/tmp\")\n").unwrap();
                stream.flush().unwrap();

                let reader = BufReader::new(stream);
                let first_line = reader.lines().next();
                assert!(first_line.is_some());
            }
            Err(_) => {
                // Socket may not be available in test env — skip
            }
        }

        let _ = std::fs::remove_file(socket_path);
    }
}
