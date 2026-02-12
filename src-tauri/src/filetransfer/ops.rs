use serde::Serialize;

use std::process::{Command, Stdio};
use tauri::Emitter;

#[cfg(unix)]
fn ssh_bin() -> &'static str { "/usr/bin/ssh" }
#[cfg(windows)]
fn ssh_bin() -> &'static str { "ssh" }
#[cfg(unix)]
fn scp_bin() -> &'static str { "/usr/bin/scp" }
#[cfg(windows)]
fn scp_bin() -> &'static str { "scp" }

#[derive(Debug, Clone, Serialize)]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub permissions: String,
}

/// Build common SSH args for host/user/port
fn ssh_base_args(user: &str, host: &str, port: Option<u16>) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
    ];
    if let Some(p) = port {
        args.push("-p".to_string());
        args.push(p.to_string());
    }
    args.push(format!("{}@{}", user, host));
    args
}

/// Run a remote command and return stdout
fn ssh_exec(user: &str, host: &str, port: Option<u16>, command: &str) -> Result<String, String> {
    let mut args = ssh_base_args(user, host, port);
    args.push("bash".to_string());

    let mut child = Command::new(ssh_bin())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("SSH failed: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(command.as_bytes()).ok();
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("SSH wait failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            return Err(format!("Remote command failed: {}", stderr.trim()));
        }
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List files in a remote directory using a structured script output
pub fn list_remote_dir(
    host: &str,
    user: &str,
    port: Option<u16>,
    path: &str,
) -> Result<Vec<RemoteFile>, String> {
    // Use a remote script that outputs structured data (pipe-delimited)
    // Format per line: name|path|type|size|modified|permissions
    let script = format!(
        r#"
DIR="{path}"
if [ ! -d "$DIR" ]; then
    echo "ERROR:NOT_A_DIRECTORY"
    exit 1
fi
cd "$DIR" 2>/dev/null || {{ echo "ERROR:CANNOT_ACCESS"; exit 1; }}
for f in .* *; do
    [ "$f" = "." ] && continue
    if [ -e "$f" ] || [ -L "$f" ]; then
        FULLPATH="$DIR/$f"
        if [ -d "$f" ]; then
            TYPE="dir"
        else
            TYPE="file"
        fi
        SIZE=$(stat -c '%s' "$f" 2>/dev/null || stat -f '%z' "$f" 2>/dev/null || echo "0")
        MOD=$(stat -c '%Y' "$f" 2>/dev/null || stat -f '%m' "$f" 2>/dev/null || echo "0")
        PERM=$(stat -c '%A' "$f" 2>/dev/null || ls -ld "$f" 2>/dev/null | awk '{{print $1}}' || echo "----------")
        echo "ENTRY:$f|$FULLPATH|$TYPE|$SIZE|$MOD|$PERM"
    fi
done
echo "DONE"
"#,
        path = path
    );

    let output = ssh_exec(user, host, port, &script)?;

    let mut files = Vec::new();

    for line in output.lines() {
        if line.starts_with("ERROR:") {
            return Err(line.strip_prefix("ERROR:").unwrap_or(line).to_string());
        }
        if let Some(entry) = line.strip_prefix("ENTRY:") {
            let parts: Vec<&str> = entry.splitn(6, '|').collect();
            if parts.len() == 6 {
                let size = parts[3].trim().parse::<u64>().unwrap_or(0);
                let timestamp = parts[4].trim().parse::<i64>().unwrap_or(0);

                // Convert epoch timestamp to human-readable
                let modified = if timestamp > 0 {
                    // Simple date formatting from epoch
                    format_epoch(timestamp)
                } else {
                    "-".to_string()
                };

                files.push(RemoteFile {
                    name: parts[0].to_string(),
                    path: parts[1].to_string(),
                    is_dir: parts[2] == "dir",
                    size,
                    modified,
                    permissions: parts[5].trim().to_string(),
                });
            }
        }
    }

    // Sort: directories first, then by name
    files.sort_by(|a, b| {
        if a.name == ".." {
            return std::cmp::Ordering::Less;
        }
        if b.name == ".." {
            return std::cmp::Ordering::Greater;
        }
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(files)
}

/// Upload a local file to remote via scp
pub fn upload_file(
    host: &str,
    user: &str,
    port: Option<u16>,
    local_path: &str,
    remote_path: &str,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let filename = std::path::Path::new(local_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| local_path.to_string());

    log::info!("Uploading {} to {}:{}", local_path, host, remote_path);

    let _ = app.emit(
        "file-transfer-progress",
        serde_json::json!({
            "filename": filename,
            "percent": 0,
            "direction": "upload",
        }),
    );

    let mut scp_args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if let Some(p) = port {
        scp_args.push("-P".to_string());
        scp_args.push(p.to_string());
    }
    scp_args.push(local_path.to_string());
    scp_args.push(format!("{}@{}:{}", user, host, remote_path));

    let output = Command::new(scp_bin())
        .args(&scp_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("SCP failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Upload failed: {}", stderr.trim()));
    }

    let _ = app.emit(
        "file-transfer-progress",
        serde_json::json!({
            "filename": filename,
            "percent": 100,
            "direction": "upload",
        }),
    );

    log::info!("Upload complete: {}", filename);
    Ok(())
}

/// Download a remote file to local via scp
pub fn download_file(
    host: &str,
    user: &str,
    port: Option<u16>,
    remote_path: &str,
    local_path: &str,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let filename = std::path::Path::new(remote_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| remote_path.to_string());

    log::info!("Downloading {}:{} to {}", host, remote_path, local_path);

    let _ = app.emit(
        "file-transfer-progress",
        serde_json::json!({
            "filename": filename,
            "percent": 0,
            "direction": "download",
        }),
    );

    let mut scp_args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if let Some(p) = port {
        scp_args.push("-P".to_string());
        scp_args.push(p.to_string());
    }
    scp_args.push(format!("{}@{}:{}", user, host, remote_path));
    scp_args.push(local_path.to_string());

    let output = Command::new(scp_bin())
        .args(&scp_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("SCP failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Download failed: {}", stderr.trim()));
    }

    let _ = app.emit(
        "file-transfer-progress",
        serde_json::json!({
            "filename": filename,
            "percent": 100,
            "direction": "download",
        }),
    );

    log::info!("Download complete: {}", filename);
    Ok(())
}

/// Create a directory on the remote host
pub fn make_remote_dir(
    host: &str,
    user: &str,
    port: Option<u16>,
    path: &str,
) -> Result<(), String> {
    let cmd = format!("mkdir -p '{}'", path.replace('\'', "'\\''"));
    ssh_exec(user, host, port, &cmd)?;
    Ok(())
}

/// Delete a file or directory on the remote host
pub fn delete_remote(
    host: &str,
    user: &str,
    port: Option<u16>,
    path: &str,
) -> Result<(), String> {
    // Safety: refuse to delete root-level critical paths
    let dangerous = ["/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib",
        "/lib64", "/opt", "/proc", "/root", "/run", "/sbin", "/srv",
        "/sys", "/tmp", "/usr", "/var"];
    let clean = path.trim_end_matches('/');
    if dangerous.contains(&clean) {
        return Err(format!("Refusing to delete critical path: {}", path));
    }

    let cmd = format!("rm -rf '{}'", path.replace('\'', "'\\''"));
    ssh_exec(user, host, port, &cmd)?;
    Ok(())
}

/// Format an epoch timestamp to a human-readable string
fn format_epoch(epoch: i64) -> String {
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                   "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    // Simple conversion — good enough for display purposes
    // Days since epoch
    let secs_per_day: i64 = 86400;
    let days = epoch / secs_per_day;
    let time_of_day = epoch % secs_per_day;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Approximate year/month/day from days since 1970-01-01
    let mut y = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    for (i, &d) in month_days.iter().enumerate() {
        if remaining < d {
            m = i;
            break;
        }
        remaining -= d;
    }
    let day = remaining + 1;

    format!("{} {:2} {:02}:{:02}", months[m], day, hours, minutes)
}
