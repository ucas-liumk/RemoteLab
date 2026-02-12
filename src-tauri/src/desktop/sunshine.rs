use std::io::BufRead;
use std::process::{Command, Stdio};
use tauri::Emitter;

/// Emit a progress event to the frontend
fn emit_progress(app: &tauri::AppHandle, percent: u8, message: &str) {
    log::info!("Sunshine progress: {}% - {}", percent, message);
    let _ = app.emit(
        "desktop-progress",
        serde_json::json!({
            "phase": "sunshine_setup",
            "percent": percent,
            "message": message,
        }),
    );
}

/// Setup Sunshine on remote host for NVENC GPU streaming.
/// Installs xorg + xfce4 + Sunshine if not present (first-time may take 3-5 min).
/// Returns the Sunshine web UI port (47990).
pub fn setup_sunshine(
    host: &str,
    user: &str,
    port: Option<u16>,
    app: &tauri::AppHandle,
) -> Result<u16, String> {
    log::info!(
        "Setting up Sunshine on {}@{}:{}",
        user,
        host,
        port.unwrap_or(22)
    );

    let script = r#"#!/bin/bash

# Smart sudo
if [ "$(id -u)" = "0" ]; then SUDO=""
else
    if sudo -n true 2>/dev/null; then SUDO="sudo"; else SUDO=""; fi
fi

# Portable port check
port_is_listening() {
    local p="$1"
    if command -v ss > /dev/null 2>&1; then
        ss -tln 2>/dev/null | grep -q ":${p} "
    elif command -v netstat > /dev/null 2>&1; then
        netstat -tln 2>/dev/null | grep -q ":${p} "
    elif [ -r /proc/net/tcp ]; then
        local hex=$(printf '%04X' "$p")
        grep -qi "00000000:${hex} " /proc/net/tcp 2>/dev/null
    else
        (echo > /dev/tcp/127.0.0.1/${p}) 2>/dev/null
    fi
}

SUNSHINE_PORT=47990

# --- Check if Sunshine is already running ---
if pgrep -f "sunshine" > /dev/null 2>&1 && port_is_listening "$SUNSHINE_PORT"; then
    echo "PROGRESS:100:Sunshine already running"
    echo "OK:READY:${SUNSHINE_PORT}"
    exit 0
fi

# --- Check if Sunshine is installed ---
NEED_INSTALL=false
if ! command -v sunshine > /dev/null 2>&1; then
    NEED_INSTALL=true
fi

NEED_DESKTOP=false
if ! command -v startxfce4 > /dev/null 2>&1; then
    NEED_DESKTOP=true
fi

if [ "$NEED_INSTALL" = "true" ] || [ "$NEED_DESKTOP" = "true" ]; then
    echo "PROGRESS:5:Updating package lists..."
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get update -qq > /dev/null 2>&1
fi

# --- Install desktop environment if needed ---
if [ "$NEED_DESKTOP" = "true" ]; then
    echo "PROGRESS:10:Installing X.org display server..."
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq \
        xorg xserver-xorg-video-dummy x11-xserver-utils \
        dbus-x11 xvfb > /dev/null 2>&1

    echo "PROGRESS:25:Installing XFCE desktop environment..."
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq \
        xfce4 xfce4-terminal xfce4-settings > /dev/null 2>&1

    echo "PROGRESS:35:Installing display libraries..."
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq \
        libvulkan1 vainfo libcap2-bin mesa-utils > /dev/null 2>&1
fi

# --- Install Sunshine if needed ---
if [ "$NEED_INSTALL" = "true" ]; then
    ARCH=$(dpkg --print-architecture 2>/dev/null || echo "amd64")
    CODENAME=$(lsb_release -cs 2>/dev/null || echo "jammy")
    SUNSHINE_DEB="/tmp/sunshine.deb"

    echo "PROGRESS:45:Downloading Sunshine (may take a while)..."
    DOWNLOADED=false

    # Try direct GitHub
    if wget -q --timeout=30 -O "$SUNSHINE_DEB" \
        "https://github.com/LizardByte/Sunshine/releases/latest/download/sunshine-${CODENAME}-${ARCH}.deb" 2>/dev/null; then
        [ -s "$SUNSHINE_DEB" ] && DOWNLOADED=true
    fi

    # Try ghproxy mirror (faster in China)
    if [ "$DOWNLOADED" = "false" ]; then
        echo "PROGRESS:48:Trying mirror download..."
        if wget -q --timeout=60 -O "$SUNSHINE_DEB" \
            "https://ghfast.top/https://github.com/LizardByte/Sunshine/releases/latest/download/sunshine-${CODENAME}-${ARCH}.deb" 2>/dev/null; then
            [ -s "$SUNSHINE_DEB" ] && DOWNLOADED=true
        fi
    fi

    # Try Ubuntu 22.04 fallback
    if [ "$DOWNLOADED" = "false" ]; then
        echo "PROGRESS:50:Trying alternative package..."
        if wget -q --timeout=60 -O "$SUNSHINE_DEB" \
            "https://ghfast.top/https://github.com/LizardByte/Sunshine/releases/latest/download/sunshine-ubuntu-22.04-amd64.deb" 2>/dev/null; then
            [ -s "$SUNSHINE_DEB" ] && DOWNLOADED=true
        fi
    fi

    if [ "$DOWNLOADED" = "false" ]; then
        echo "ERROR:DOWNLOAD_FAILED"
        echo "HINT:Cannot download Sunshine. Network may be restricted."
        exit 1
    fi

    echo "PROGRESS:60:Installing Sunshine..."
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq "$SUNSHINE_DEB" > /dev/null 2>&1 || {
        $SUDO apt-get install -f -y -qq > /dev/null 2>&1
        DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq "$SUNSHINE_DEB" > /dev/null 2>&1
    }

    if ! command -v sunshine > /dev/null 2>&1; then
        echo "ERROR:INSTALL_FAILED"
        echo "HINT:Sunshine package installation failed."
        exit 1
    fi

    # Set capabilities
    SUNSHINE_BIN=$(readlink -f $(which sunshine) 2>/dev/null || echo "/usr/bin/sunshine")
    $SUDO setcap cap_sys_admin+p "$SUNSHINE_BIN" 2>/dev/null || true
else
    echo "PROGRESS:60:Sunshine already installed"
fi

# --- Setup display ---
echo "PROGRESS:70:Setting up virtual display..."
DISPLAY_NUM=""
for d in /tmp/.X11-unix/X*; do
    if [ -e "$d" ]; then
        num=$(echo "$d" | sed 's|/tmp/.X11-unix/X||')
        DISPLAY_NUM="$num"
        break
    fi
done

if [ -z "$DISPLAY_NUM" ]; then
    DISPLAY_NUM="10"
    if ! pgrep -f "Xvfb :${DISPLAY_NUM}" > /dev/null 2>&1; then
        if command -v Xvfb > /dev/null 2>&1; then
            nohup Xvfb ":${DISPLAY_NUM}" -screen 0 1920x1080x24 </dev/null >/dev/null 2>&1 &
            disown
            sleep 1
        fi
    fi
fi

if ! pgrep -f "xfce4-session\|xfdesktop" > /dev/null 2>&1; then
    if command -v startxfce4 > /dev/null 2>&1; then
        echo "PROGRESS:75:Starting XFCE desktop session..."
        DISPLAY=":${DISPLAY_NUM}" nohup startxfce4 </dev/null >/dev/null 2>&1 &
        disown
        sleep 3
    fi
fi

# --- Configure Sunshine ---
echo "PROGRESS:80:Configuring Sunshine..."
SUNSHINE_CONF_DIR="$HOME/.config/sunshine"
mkdir -p "$SUNSHINE_CONF_DIR"
cat > "$SUNSHINE_CONF_DIR/sunshine.conf" << 'SCONF'
origin_web_ui_allowed = wan
upnp = off
encoder = nvenc
min_fps_factor = 1
channels = []
SCONF

# Generate random credentials for Sunshine web UI
SUN_PASS=$(head -c 16 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 12)
sunshine --creds sunshine "$SUN_PASS" 2>/dev/null || true

# --- Start Sunshine ---
if ! pgrep -f "sunshine" > /dev/null 2>&1; then
    echo "PROGRESS:90:Starting Sunshine server..."
    DISPLAY=":${DISPLAY_NUM}" nohup sunshine > /tmp/sunshine.log 2>&1 </dev/null &
    disown
    sleep 3
fi

# --- Verify ---
echo "PROGRESS:95:Verifying Sunshine..."
if ! pgrep -f "sunshine" > /dev/null 2>&1; then
    echo "ERROR:START_FAILED"
    echo "HINT:Sunshine failed to start. Check /tmp/sunshine.log"
    echo "DEBUG:$(tail -5 /tmp/sunshine.log 2>/dev/null)"
    exit 1
fi

# Wait for port
for _i in 1 2 3 4 5; do
    if port_is_listening "$SUNSHINE_PORT"; then
        echo "PROGRESS:100:Sunshine ready!"
        echo "OK:READY:${SUNSHINE_PORT}"
        exit 0
    fi
    sleep 1
done

echo "ERROR:PORT_NOT_READY"
echo "DEBUG:$(tail -5 /tmp/sunshine.log 2>/dev/null)"
exit 1
"#;

    let mut ssh_args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=15".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
    ];
    if let Some(p) = port {
        ssh_args.push("-p".to_string());
        ssh_args.push(p.to_string());
    }
    ssh_args.push(format!("{}@{}", user, host));
    ssh_args.push("bash".to_string());

    #[cfg(unix)]
    let ssh_bin = "/usr/bin/ssh";
    #[cfg(windows)]
    let ssh_bin = "ssh";
    let mut child = Command::new(ssh_bin)
        .args(&ssh_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("SSH failed: {}", e))?;

    // Write script to stdin and close it
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(script.as_bytes()).ok();
        // drop closes stdin, signaling end of input
    }

    let mut result_line = String::new();

    // Read stdout line by line, emitting progress events
    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            log::debug!("sunshine ssh: {}", line);
            if line.starts_with("PROGRESS:") {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() == 3 {
                    if let Ok(pct) = parts[1].parse::<u8>() {
                        emit_progress(app, pct, parts[2]);
                    }
                }
            } else if line.starts_with("OK:") || line.starts_with("ERROR:") {
                result_line = line;
            }
        }
    }

    let status = child.wait().map_err(|e| format!("SSH process error: {}", e))?;
    log::info!("Sunshine SSH exit: {:?}, result: {}", status.code(), result_line);

    if result_line.starts_with("OK:") {
        let port: u16 = result_line
            .rsplit(':')
            .next()
            .and_then(|p| p.trim().parse().ok())
            .unwrap_or(47990);
        Ok(port)
    } else if result_line.contains("DOWNLOAD_FAILED") {
        Err("Cannot download Sunshine. Network may be restricted.".to_string())
    } else if result_line.contains("INSTALL_FAILED") {
        Err("Sunshine installation failed.".to_string())
    } else if result_line.contains("START_FAILED") {
        Err("Sunshine failed to start.".to_string())
    } else {
        Err(format!("Sunshine setup failed: {}", result_line))
    }
}
