use futures_util::{SinkExt, StreamExt};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

/// Manages SSH tunnel + WebSocket-to-TCP proxy for VNC connections
pub struct VncProxy {
    /// SSH tunnel process
    ssh_process: Mutex<Option<Child>>,
    /// Local VNC port (SSH tunnel endpoint)
    vnc_port: Mutex<u16>,
    /// WebSocket proxy port
    ws_port: Mutex<u16>,
    /// Proxy running flag
    running: Mutex<bool>,
}

impl VncProxy {
    pub fn new() -> Self {
        Self {
            ssh_process: Mutex::new(None),
            vnc_port: Mutex::new(0),
            ws_port: Mutex::new(0),
            running: Mutex::new(false),
        }
    }

    /// Find an available port
    fn find_port() -> Result<u16, String> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to find port: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get addr: {}", e))?
            .port();
        Ok(port)
    }

    /// Auto-setup VNC server on remote host via SSH
    /// Installs x11vnc if needed and starts it
    pub fn setup_remote_vnc(&self, host: &str, user: &str) -> Result<u16, String> {
        log::info!("Setting up VNC server on {}@{}...", user, host);

        // Run a setup script on the remote host via SSH
        // This will: check for x11vnc, install if needed, find a display, start x11vnc
        let setup_script = r#"
set -e

# Find a working display
DISPLAY_NUM=""
for d in /tmp/.X11-unix/X*; do
    if [ -e "$d" ]; then
        num=$(echo "$d" | sed 's|/tmp/.X11-unix/X||')
        DISPLAY_NUM="$num"
        break
    fi
done

# If no X display, try to check for Wayland or headless
if [ -z "$DISPLAY_NUM" ]; then
    echo "ERROR:NO_DISPLAY"
    exit 1
fi

DISPLAY=":$DISPLAY_NUM"

# Check if x11vnc is already running and serving this display
if pgrep -f "x11vnc.*display $DISPLAY" > /dev/null 2>&1; then
    # Already running, get port
    PORT=$(ss -tlnp 2>/dev/null | grep x11vnc | grep -o ':[0-9]*' | head -1 | tr -d ':')
    if [ -z "$PORT" ]; then PORT=5900; fi
    echo "OK:ALREADY_RUNNING:$PORT"
    exit 0
fi

# Install x11vnc if not present
if ! command -v x11vnc > /dev/null 2>&1; then
    echo "INSTALLING_X11VNC"
    if command -v apt-get > /dev/null 2>&1; then
        sudo DEBIAN_FRONTEND=noninteractive apt-get install -y x11vnc > /dev/null 2>&1
    elif command -v yum > /dev/null 2>&1; then
        sudo yum install -y x11vnc > /dev/null 2>&1
    elif command -v pacman > /dev/null 2>&1; then
        sudo pacman -S --noconfirm x11vnc > /dev/null 2>&1
    else
        echo "ERROR:CANNOT_INSTALL"
        exit 1
    fi
fi

# Kill stale x11vnc instances
killall x11vnc 2>/dev/null || true
sleep 0.5

# Find a free port starting from 5900
VNC_PORT=5900
while ss -tln 2>/dev/null | grep -q ":$VNC_PORT "; do
    VNC_PORT=$((VNC_PORT + 1))
    if [ $VNC_PORT -gt 5910 ]; then
        echo "ERROR:NO_FREE_PORT"
        exit 1
    fi
done

# Start x11vnc in background
x11vnc -display "$DISPLAY" -rfbport "$VNC_PORT" -forever -nopw -shared -bg -o /tmp/x11vnc.log 2>/dev/null

# Verify it started
sleep 1
if ss -tln 2>/dev/null | grep -q ":$VNC_PORT "; then
    echo "OK:STARTED:$VNC_PORT"
else
    echo "ERROR:START_FAILED"
    exit 1
fi
"#;

        let output = Command::new("/usr/bin/ssh")
            .args([
                "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ConnectTimeout=10",
                "-o", "ServerAliveInterval=30",
                &format!("{}@{}", user, host),
                "bash",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(setup_script.as_bytes()).ok();
                }
                child.wait_with_output()
            })
            .map_err(|e| format!("Failed to SSH for VNC setup: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::info!("VNC setup stdout: {}", stdout);
        if !stderr.is_empty() {
            log::warn!("VNC setup stderr: {}", stderr);
        }

        // Parse the result
        let last_line = stdout.lines().last().unwrap_or("");

        if last_line.starts_with("OK:") {
            // Extract port from "OK:STARTED:5900" or "OK:ALREADY_RUNNING:5900"
            let port: u16 = last_line
                .rsplit(':')
                .next()
                .and_then(|p| p.trim().parse().ok())
                .unwrap_or(5900);
            log::info!("VNC server ready on remote port {}", port);
            Ok(port)
        } else if last_line.contains("NO_DISPLAY") {
            Err("No X11 display found on remote host. The remote machine may not have a graphical session running.".to_string())
        } else if last_line.contains("CANNOT_INSTALL") {
            Err("Could not install x11vnc on remote host. Unknown package manager.".to_string())
        } else if last_line.contains("START_FAILED") {
            Err("x11vnc failed to start. Check /tmp/x11vnc.log on the remote host.".to_string())
        } else {
            Err(format!("VNC setup failed: {}", if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() }))
        }
    }

    /// Start SSH tunnel: ssh -L <local>:localhost:<remote_vnc_port> -N user@host
    pub fn start_tunnel(&self, host: &str, user: &str, vnc_port: u16, ssh_port: Option<u16>) -> Result<u16, String> {
        // Stop any existing tunnel
        self.stop();

        let local_port = Self::find_port()?;

        let mut ssh_args = vec![
            "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
            "-o".to_string(), "ServerAliveInterval=30".to_string(),
            "-o".to_string(), "ExitOnForwardFailure=yes".to_string(),
            "-o".to_string(), "ConnectTimeout=10".to_string(),
        ];
        if let Some(p) = ssh_port {
            ssh_args.push("-p".to_string());
            ssh_args.push(p.to_string());
        }
        ssh_args.push("-N".to_string());
        ssh_args.push("-L".to_string());
        ssh_args.push(format!("{}:localhost:{}", local_port, vnc_port));
        ssh_args.push(format!("{}@{}", user, host));

        let child = Command::new("/usr/bin/ssh")
            .args(&ssh_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start SSH tunnel: {}", e))?;

        *self.ssh_process.lock().unwrap() = Some(child);
        *self.vnc_port.lock().unwrap() = local_port;

        // Wait for tunnel to establish with retries
        let mut connected = false;
        for attempt in 0..6 {
            std::thread::sleep(std::time::Duration::from_millis(if attempt == 0 { 800 } else { 1000 }));
            match std::net::TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", local_port).parse().unwrap(),
                std::time::Duration::from_secs(2),
            ) {
                Ok(_) => {
                    connected = true;
                    log::info!("SSH tunnel established on port {} (attempt {})", local_port, attempt + 1);
                    break;
                }
                Err(e) => {
                    log::debug!("Tunnel check attempt {}: {}", attempt + 1, e);
                }
            }
        }

        if !connected {
            self.stop();
            return Err("SSH tunnel failed to establish after 6 attempts. Remote VNC server may not be ready.".to_string());
        }

        Ok(local_port)
    }

    /// Start WebSocket-to-TCP proxy
    pub async fn start_ws_proxy(&self) -> Result<u16, String> {
        let vnc_port = *self.vnc_port.lock().unwrap();
        if vnc_port == 0 {
            return Err("SSH tunnel not started".to_string());
        }

        let ws_port = Self::find_port()?;
        *self.ws_port.lock().unwrap() = ws_port;
        *self.running.lock().unwrap() = true;

        let listener = TcpListener::bind(format!("127.0.0.1:{}", ws_port))
            .await
            .map_err(|e| format!("Failed to bind WS port: {}", e))?;

        // Spawn proxy task
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let vnc_port = vnc_port;
                tokio::spawn(async move {
                    if let Err(e) = handle_ws_connection(stream, vnc_port).await {
                        log::error!("WS proxy error: {}", e);
                    }
                });
            }
        });

        Ok(ws_port)
    }

    /// Full VNC connection: auto-setup remote VNC server + tunnel + WS proxy
    pub async fn connect(
        &self,
        host: &str,
        user: &str,
        remote_vnc_port: Option<u16>,
        ssh_port: Option<u16>,
    ) -> Result<u16, String> {
        // Step 1: Auto-setup VNC server on remote host (install x11vnc if needed)
        let vnc_port = if let Some(port) = remote_vnc_port {
            port
        } else {
            self.setup_remote_vnc(host, user)?
        };

        // Step 2: SSH tunnel
        self.start_tunnel(host, user, vnc_port, ssh_port)?;

        // Step 3: WebSocket proxy
        self.start_ws_proxy().await
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
        if let Some(mut child) = self.ssh_process.lock().unwrap().take() {
            child.kill().ok();
        }
        *self.vnc_port.lock().unwrap() = 0;
        *self.ws_port.lock().unwrap() = 0;
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
}

async fn handle_ws_connection(stream: TcpStream, vnc_port: u16) -> Result<(), String> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| format!("WS accept failed: {}", e))?;

    let vnc_stream = TcpStream::connect(format!("127.0.0.1:{}", vnc_port))
        .await
        .map_err(|e| format!("VNC connect failed: {}", e))?;

    let (mut vnc_read, mut vnc_write) = vnc_stream.into_split();
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // WebSocket → VNC TCP
    let ws_to_vnc = tokio::spawn(async move {
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if vnc_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // VNC TCP → WebSocket (64KB buffer for better throughput)
    let vnc_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            match vnc_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if ws_write
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Wait for either direction to finish
    tokio::select! {
        _ = ws_to_vnc => {},
        _ = vnc_to_ws => {},
    }

    Ok(())
}
