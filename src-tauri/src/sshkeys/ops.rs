use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(unix)]
fn ssh_bin() -> &'static str { "/usr/bin/ssh" }
#[cfg(windows)]
fn ssh_bin() -> &'static str { "ssh" }

#[cfg(unix)]
fn ssh_keygen_bin() -> &'static str { "/usr/bin/ssh-keygen" }
#[cfg(windows)]
fn ssh_keygen_bin() -> &'static str { "ssh-keygen" }

#[derive(Debug, Clone, serde::Serialize)]
pub struct SshKeyInfo {
    pub name: String,
    pub path: String,
    pub key_type: String,
    pub has_public: bool,
    pub public_key: Option<String>,
    pub fingerprint: Option<String>,
}

/// Return the ~/.ssh directory path
fn ssh_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".ssh"))
        .ok_or_else(|| "Could not determine home directory".to_string())
}

/// Detect key type from the private key file content or public key content
fn detect_key_type(private_path: &PathBuf) -> String {
    // The public key file is <name>.pub alongside the private key
    let pub_path = PathBuf::from(format!("{}.pub", private_path.display()));

    if let Ok(content) = fs::read_to_string(&pub_path) {
        let content = content.trim();
        if content.starts_with("ssh-ed25519") {
            return "ed25519".to_string();
        } else if content.starts_with("ssh-rsa") {
            return "rsa".to_string();
        } else if content.starts_with("ecdsa-") {
            return "ecdsa".to_string();
        } else if content.starts_with("ssh-dss") {
            return "dsa".to_string();
        }
    }

    // Fall back to reading the private key header
    if let Ok(content) = fs::read_to_string(private_path) {
        let first_line = content.lines().next().unwrap_or("");
        if first_line.contains("OPENSSH PRIVATE KEY") {
            // Generic OpenSSH format; try ssh-keygen -l to determine type
            if let Ok(output) = Command::new(ssh_keygen_bin())
                .args(["-l", "-f", &private_path.to_string_lossy()])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lower = stdout.to_lowercase();
                if lower.contains("ed25519") {
                    return "ed25519".to_string();
                } else if lower.contains("rsa") {
                    return "rsa".to_string();
                } else if lower.contains("ecdsa") {
                    return "ecdsa".to_string();
                } else if lower.contains("dsa") {
                    return "dsa".to_string();
                }
            }
            return "openssh".to_string();
        } else if first_line.contains("RSA PRIVATE KEY") {
            return "rsa".to_string();
        } else if first_line.contains("EC PRIVATE KEY") {
            return "ecdsa".to_string();
        } else if first_line.contains("DSA PRIVATE KEY") {
            return "dsa".to_string();
        }
    }

    "unknown".to_string()
}

/// Get the fingerprint of a key using ssh-keygen -l
fn get_fingerprint(key_path: &PathBuf) -> Option<String> {
    let output = Command::new(ssh_keygen_bin())
        .args(["-l", "-f", &key_path.to_string_lossy()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Output format: "256 SHA256:xxxx comment (TYPE)"
        // Return the whole line which is informative enough
        Some(stdout)
    } else {
        None
    }
}

/// Files to skip when scanning ~/.ssh/
const SKIP_FILES: &[&str] = &[
    "known_hosts",
    "known_hosts.old",
    "config",
    "authorized_keys",
    "environment",
    "rc",
];

/// List all SSH private keys in ~/.ssh/
pub fn list_ssh_keys() -> Result<Vec<SshKeyInfo>, String> {
    let ssh_path = ssh_dir()?;

    if !ssh_path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&ssh_path)
        .map_err(|e| format!("Failed to read ~/.ssh: {}", e))?;

    let mut keys = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        let file_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        // Skip .pub files — we only list private keys
        if file_name.ends_with(".pub") {
            continue;
        }

        // Skip known non-key files
        if SKIP_FILES.contains(&file_name.as_str()) {
            continue;
        }

        // Skip hidden files (e.g. .DS_Store)
        if file_name.starts_with('.') {
            continue;
        }

        // Verify this looks like a private key by checking the first line
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let first_line = content.lines().next().unwrap_or("");
        if !first_line.contains("PRIVATE KEY") && !first_line.contains("BEGIN OPENSSH") {
            continue;
        }

        let pub_path = PathBuf::from(format!("{}.pub", path.display()));
        let has_public = pub_path.exists();
        let public_key = if has_public {
            fs::read_to_string(&pub_path).ok().map(|s| s.trim().to_string())
        } else {
            None
        };

        let key_type = detect_key_type(&path);
        let fingerprint = get_fingerprint(&path);

        keys.push(SshKeyInfo {
            name: file_name,
            path: path.to_string_lossy().to_string(),
            key_type,
            has_public,
            public_key,
            fingerprint,
        });
    }

    // Sort by name for stable ordering
    keys.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(keys)
}

/// Generate a new SSH key using ssh-keygen
pub fn generate_key(name: &str, passphrase: &str) -> Result<SshKeyInfo, String> {
    // Validate name: no path separators, no spaces, reasonable length
    if name.is_empty() {
        return Err("Key name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains(' ') {
        return Err("Key name must not contain slashes or spaces".to_string());
    }
    if name.len() > 64 {
        return Err("Key name is too long (max 64 characters)".to_string());
    }

    let ssh_path = ssh_dir()?;

    // Create ~/.ssh if it doesn't exist
    if !ssh_path.exists() {
        fs::create_dir_all(&ssh_path)
            .map_err(|e| format!("Failed to create ~/.ssh: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&ssh_path, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Failed to set ~/.ssh permissions: {}", e))?;
        }
    }

    let key_path = ssh_path.join(name);
    if key_path.exists() {
        return Err(format!("Key '{}' already exists", name));
    }

    let comment = format!("{}@remotelab", name);

    let output = Command::new(ssh_keygen_bin())
        .args([
            "-t", "ed25519",
            "-f", &key_path.to_string_lossy(),
            "-N", passphrase,
            "-C", &comment,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ssh-keygen failed to start: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ssh-keygen failed: {}", stderr.trim()));
    }

    // Read the generated key info
    let pub_path = PathBuf::from(format!("{}.pub", key_path.display()));
    let has_public = pub_path.exists();
    let public_key = if has_public {
        fs::read_to_string(&pub_path).ok().map(|s| s.trim().to_string())
    } else {
        None
    };
    let fingerprint = get_fingerprint(&key_path);

    log::info!("Generated SSH key: {}", name);

    Ok(SshKeyInfo {
        name: name.to_string(),
        path: key_path.to_string_lossy().to_string(),
        key_type: "ed25519".to_string(),
        has_public,
        public_key,
        fingerprint,
    })
}

/// Copy a public key to a remote host's authorized_keys
pub fn copy_key_to_remote(
    key_path: &str,
    host: &str,
    user: &str,
    port: Option<u16>,
) -> Result<(), String> {
    let pub_path = if key_path.ends_with(".pub") {
        PathBuf::from(key_path)
    } else {
        PathBuf::from(format!("{}.pub", key_path))
    };

    if !pub_path.exists() {
        return Err(format!(
            "Public key not found: {}",
            pub_path.display()
        ));
    }

    let public_key = fs::read_to_string(&pub_path)
        .map_err(|e| format!("Failed to read public key: {}", e))?
        .trim()
        .to_string();

    if public_key.is_empty() {
        return Err("Public key file is empty".to_string());
    }

    // Build SSH args
    let mut args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if let Some(p) = port {
        args.push("-p".to_string());
        args.push(p.to_string());
    }
    args.push(format!("{}@{}", user, host));

    // Remote command: create ~/.ssh if needed, append key if not already present
    let escaped_key = public_key.replace('\'', "'\\''");
    let remote_cmd = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
         grep -qF '{}' ~/.ssh/authorized_keys 2>/dev/null || echo '{}' >> ~/.ssh/authorized_keys",
        escaped_key, escaped_key
    );
    args.push(remote_cmd);

    log::info!("Copying SSH key to {}@{}", user, host);

    let output = Command::new(ssh_bin())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("SSH failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to copy key to remote: {}", stderr.trim()));
    }

    log::info!("SSH key copied to {}@{}", user, host);
    Ok(())
}
