use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{Emitter, Manager};

struct PtySession {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub struct TerminalManager {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn open_session(
        &self,
        session_id: &str,
        host: &str,
        user: &str,
        port: Option<u16>,
        app_handle: tauri::AppHandle,
    ) -> Result<(), String> {
        log::info!("Opening SSH session {} to {}@{}:{}", session_id, user, host, port.unwrap_or(22));

        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                log::error!("Failed to open PTY: {}", e);
                format!("Failed to open PTY: {}", e)
            })?;

        let home_dir = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()));

        #[cfg(unix)]
        let ssh_bin = "/usr/bin/ssh";
        #[cfg(windows)]
        let ssh_bin = "ssh";
        let mut cmd = CommandBuilder::new(ssh_bin);
        let mut ssh_args = vec![
            "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
            "-o".to_string(), "ServerAliveInterval=30".to_string(),
            "-o".to_string(), "ConnectTimeout=10".to_string(),
            "-tt".to_string(),
        ];
        if let Some(p) = port {
            ssh_args.push("-p".to_string());
            ssh_args.push(p.to_string());
        }
        ssh_args.push(format!("{}@{}", user, host));
        cmd.args(ssh_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());

        cmd.env("HOME", &home_dir);
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", "en_US.UTF-8");
        #[cfg(target_os = "macos")]
        cmd.env("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
        #[cfg(target_os = "linux")]
        cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
        #[cfg(target_os = "windows")]
        if let Ok(path) = std::env::var("PATH") { cmd.env("PATH", &path); }

        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", &sock);
        }

        cmd.cwd(&home_dir);

        log::info!("Spawning SSH to {}@{}", user, host);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| {
                log::error!("Failed to spawn SSH: {}", e);
                format!("Failed to spawn ssh: {}", e)
            })?;

        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY writer: {}", e))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to get PTY reader: {}", e))?;

        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));

        let session = PtySession {
            master: master.clone(),
            writer: writer.clone(),
            _child: child,
        };

        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.to_string(), session);

        // Spawn reader thread — forwards PTY output to frontend via Tauri events
        let sid = session_id.to_string();
        thread::spawn(move || {
            log::info!("PTY reader thread started for {}", sid);
            let mut buf = [0u8; 4096];
            let mut total_bytes = 0usize;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        log::info!("PTY EOF for {} (total {} bytes)", sid, total_bytes);
                        break;
                    }
                    Ok(n) => {
                        total_bytes += n;
                        // Encode as base64 to avoid JSON serialization issues with Vec<u8>
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                        let event_name = format!("terminal-output-{}", sid);
                        // Try emit_to main window first, fall back to broadcast emit
                        let emit_result = if let Some(window) = app_handle.get_webview_window("main") {
                            window.emit(&event_name, &b64)
                        } else {
                            app_handle.emit(&event_name, &b64)
                        };
                        match emit_result {
                            Ok(_) => {},
                            Err(e) => {
                                log::error!("emit failed for {} ({} bytes): {:?}", sid, n, e);
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                // Retry once with broadcast
                                if app_handle.emit(&event_name, &b64).is_err() {
                                    log::error!("emit retry also failed, stopping reader for {}", sid);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("PTY read error for {}: {}", sid, e);
                        break;
                    }
                }
            }
            app_handle
                .emit(&format!("terminal-exit-{}", sid), ())
                .ok();
            log::info!("PTY reader thread ended for {}", sid);
        });

        log::info!("SSH session {} setup complete", session_id);
        Ok(())
    }

    pub fn write_session(&self, session_id: &str, data: &[u8]) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(session_id).ok_or("Session not found")?;
        let mut writer = session.writer.lock().unwrap();
        writer
            .write_all(data)
            .map_err(|e| format!("Write failed: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Flush failed: {}", e))?;
        Ok(())
    }

    pub fn resize_session(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(session_id).ok_or("Session not found")?;
        let master = session.master.lock().unwrap();
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Resize failed: {}", e))?;
        Ok(())
    }

    pub fn close_session(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(_session) = sessions.remove(session_id) {
            log::info!("Closed SSH session {}", session_id);
        }
        Ok(())
    }
}
