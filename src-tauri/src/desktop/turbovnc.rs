use std::io::BufRead;
use std::process::{Command, Stdio};
use tauri::Emitter;

/// Setup VNC on remote host for remote desktop.
/// Strategy:
///   1. Check for already-running VNC server → reuse it
///   2. Find or install x11vnc + xfce4 (userspace install if no root)
///   3. Start x11vnc with -create (auto-creates virtual display via Xvfb)
///      or attach to existing user X display
/// Returns the VNC port number on the remote host.
pub fn setup_turbovnc(host: &str, user: &str, port: Option<u16>, app: &tauri::AppHandle) -> Result<u16, String> {
    log::info!("Setting up VNC on {}@{}:{}", user, host, port.unwrap_or(22));

    let script = r#"#!/bin/bash

X11VNC_BIN=""

# Smart sudo: if we ARE root, no sudo needed
if [ "$(id -u)" = "0" ]; then
    SUDO=""
else
    if sudo -n true 2>/dev/null; then
        SUDO="sudo"
    else
        SUDO=""  # no sudo available, try userspace
    fi
fi

CAN_INSTALL=false
if [ "$(id -u)" = "0" ] || sudo -n true 2>/dev/null; then
    CAN_INSTALL=true
fi

# Portable port check: works with ss, netstat, /proc, or /dev/tcp
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

# Helper: validate x11vnc binary actually works
validate_x11vnc() {
    "$1" -version > /dev/null 2>&1
}

echo "PROGRESS:10:Checking for running VNC server..."
# ---- Step 0: Check for already-running VNC ----
for proc in x11vnc Xvnc x0vncserver; do
    if pgrep -x "$proc" > /dev/null 2>&1; then
        # Try to find which port by testing common VNC ports
        for p in $(seq 5900 5910); do
            if port_is_listening "$p"; then
                echo "OK:${proc}_RUNNING:$p"
                exit 0
            fi
        done
    fi
done

echo "PROGRESS:20:Finding or installing x11vnc..."
# ---- Step 1: Find or install x11vnc ----

# Check system x11vnc
if command -v x11vnc > /dev/null 2>&1 && validate_x11vnc "x11vnc"; then
    X11VNC_BIN="x11vnc"
fi

# Check userspace x11vnc (from previous install) — validate it works
if [ -z "$X11VNC_BIN" ] && [ -x "$HOME/.local/bin/x11vnc" ]; then
    export LD_LIBRARY_PATH="$HOME/.local/lib:${LD_LIBRARY_PATH:-}"
    if validate_x11vnc "$HOME/.local/bin/x11vnc"; then
        X11VNC_BIN="$HOME/.local/bin/x11vnc"
    else
        rm -f "$HOME/.local/bin/x11vnc" 2>/dev/null
    fi
fi

# Try system install (root or passwordless sudo)
if [ -z "$X11VNC_BIN" ] && [ "$CAN_INSTALL" = "true" ]; then
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get update -qq > /dev/null 2>&1
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq x11vnc xvfb > /dev/null 2>&1
    if command -v x11vnc > /dev/null 2>&1 && validate_x11vnc "x11vnc"; then
        X11VNC_BIN="x11vnc"
    fi
fi

# Userspace install (no root needed) — download deb and extract binary + libs
if [ -z "$X11VNC_BIN" ]; then
    mkdir -p "$HOME/.local/bin" "$HOME/.local/lib" /tmp/_x11vnc_dl
    cd /tmp/_x11vnc_dl

    # Download x11vnc and its key dependency libvncserver
    apt-get download x11vnc libvncclient1 libvncserver1 > /dev/null 2>&1
    for deb in *.deb; do
        [ -s "$deb" ] && dpkg-deb -x "$deb" /tmp/_x11vnc_ext > /dev/null 2>&1
    done

    if [ -x /tmp/_x11vnc_ext/usr/bin/x11vnc ]; then
        cp /tmp/_x11vnc_ext/usr/bin/x11vnc "$HOME/.local/bin/x11vnc"
        chmod +x "$HOME/.local/bin/x11vnc"
        cp /tmp/_x11vnc_ext/usr/lib/*/libvnc*.so* "$HOME/.local/lib/" 2>/dev/null
        cp /tmp/_x11vnc_ext/usr/lib/libvnc*.so* "$HOME/.local/lib/" 2>/dev/null

        export LD_LIBRARY_PATH="$HOME/.local/lib:${LD_LIBRARY_PATH:-}"
        if validate_x11vnc "$HOME/.local/bin/x11vnc"; then
            X11VNC_BIN="$HOME/.local/bin/x11vnc"
        fi
    fi

    rm -rf /tmp/_x11vnc_dl /tmp/_x11vnc_ext 2>/dev/null
    cd "$HOME"
fi

if [ -z "$X11VNC_BIN" ]; then
    echo "ERROR:CANNOT_GET_X11VNC"
    echo "HINT:Please run on the server: apt install -y x11vnc xvfb"
    exit 1
fi

echo "PROGRESS:50:Ensuring virtual display support..."
# ---- Step 1b: Ensure Xvfb is available (needed for -create mode) ----
if ! command -v Xvfb > /dev/null 2>&1; then
    if [ "$CAN_INSTALL" = "true" ]; then
        DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq xvfb > /dev/null 2>&1
    fi
fi

# ---- Step 2: Find a free VNC port ----
VNC_PORT=5900
while port_is_listening "$VNC_PORT"; do
    VNC_PORT=$((VNC_PORT + 1))
    if [ "$VNC_PORT" -gt 5920 ]; then
        echo "ERROR:NO_FREE_PORT"
        exit 1
    fi
done

# ---- Step 3: Find user's own X display (not gdm) ----
USER_DISPLAY=""

# Check xauth for user-owned displays
for cookie_display in $(xauth list 2>/dev/null | awk '{print $1}' | grep "unix:" | sed 's/.*unix://'); do
    if [ -e "/tmp/.X11-unix/X${cookie_display}" ]; then
        if DISPLAY=":${cookie_display}" xdpyinfo > /dev/null 2>&1; then
            USER_DISPLAY="$cookie_display"
            break
        fi
    fi
done

echo "PROGRESS:70:Starting VNC server..."
# ---- Step 4: Start x11vnc ----
killall x11vnc 2>/dev/null || true
sleep 0.3

# Set LD_LIBRARY_PATH for userspace-installed x11vnc
export LD_LIBRARY_PATH="$HOME/.local/lib:${LD_LIBRARY_PATH:-}"

# Ensure XFCE desktop is available for a proper desktop experience
if ! command -v startxfce4 > /dev/null 2>&1; then
    if [ "$CAN_INSTALL" = "true" ]; then
        echo "PROGRESS:72:Installing desktop environment..."
        DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq \
            xfce4 xfce4-terminal dbus-x11 > /dev/null 2>&1
    fi
fi

# NOTE: Do NOT use x11vnc's -bg flag — it forks and inherits SSH FDs,
# causing the SSH session to hang in Docker containers.
# Instead, use nohup + shell backgrounding + disown.

# Performance-optimized x11vnc flags:
#   -ncache 10   : client-side pixmap caching (reduces bandwidth hugely for scrolling/window moves)
#   -ncache_cr   : use CopyRect with cache for even better caching
#   -threads     : threaded encoding for better CPU utilization
#   -noxdamage   : avoid X Damage overhead on virtual displays (more reliable in Xvfb)
#   -cursor arrow: ensure cursor is always visible
VNC_OPTS="-forever -shared -nopw -ncache 10 -ncache_cr -threads -noxdamage -cursor arrow"

if [ -n "$USER_DISPLAY" ]; then
    # Attach to user's existing X display
    nohup "$X11VNC_BIN" -display ":${USER_DISPLAY}" \
        -auth "$HOME/.Xauthority" \
        -rfbport "${VNC_PORT}" \
        $VNC_OPTS \
        -o /tmp/x11vnc.log </dev/null >/dev/null 2>/tmp/x11vnc_err.log &
    disown

    for _i in 1 2 3 4; do
        sleep 1
        if port_is_listening "$VNC_PORT"; then
            echo "OK:X11VNC_ATTACH:${VNC_PORT}"
            exit 0
        fi
    done
fi

# Set resolution for the auto-created virtual display (1920x1080 24-bit color)
export X11VNC_CREATE_GEOM="1920x1080x24"

# Start XFCE in the new display if available (x11vnc -create will set DISPLAY)
export X11VNC_CREATE_STARTING_DESKTOP_SESSION="startxfce4"

# No user display or attach failed → use -create (auto Xvfb + session)
nohup "$X11VNC_BIN" -create \
    -rfbport "${VNC_PORT}" \
    $VNC_OPTS \
    -o /tmp/x11vnc.log </dev/null >/dev/null 2>/tmp/x11vnc_err.log &
disown

for _i in 1 2 3 4 5; do
    sleep 1
    if port_is_listening "$VNC_PORT"; then
        echo "OK:X11VNC_CREATE:${VNC_PORT}"
        exit 0
    fi
done

# Last resort check log
echo "ERROR:X11VNC_START_FAILED"
echo "DEBUG:$(tail -5 /tmp/x11vnc.log 2>/dev/null)"
echo "DEBUG:$(tail -3 /tmp/x11vnc_err.log 2>/dev/null)"
exit 1
"#;

    let mut ssh_args = vec![
        "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(), "ConnectTimeout=15".to_string(),
        "-o".to_string(), "ServerAliveInterval=30".to_string(),
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

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(script.as_bytes()).ok();
    }

    let mut result_line = String::new();
    let mut error_lines = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            log::debug!("vnc ssh: {}", line);
            if line.starts_with("PROGRESS:") {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() == 3 {
                    if let Ok(pct) = parts[1].parse::<u8>() {
                        let _ = app.emit(
                            "desktop-progress",
                            serde_json::json!({
                                "phase": "vnc_setup",
                                "percent": pct,
                                "message": parts[2],
                            }),
                        );
                    }
                }
            } else if line.starts_with("OK:") {
                result_line = line;
            } else if line.starts_with("ERROR:") || line.starts_with("HINT:") || line.starts_with("DEBUG:") {
                error_lines.push(line);
            }
        }
    }

    let _ = child.wait();

    if result_line.starts_with("OK:") {
        let port: u16 = result_line
            .rsplit(':')
            .next()
            .and_then(|p| p.trim().parse().ok())
            .unwrap_or(5901);
        log::info!("VNC ready on remote port {}", port);
        return Ok(port);
    }

    let hints: Vec<String> = error_lines
        .iter()
        .filter(|l| l.starts_with("HINT:"))
        .map(|h| h.strip_prefix("HINT:").unwrap_or(h).trim().to_string())
        .collect();

    if !hints.is_empty() {
        return Err(hints.join("\n"));
    }

    Err(error_lines.join("; "))
}
