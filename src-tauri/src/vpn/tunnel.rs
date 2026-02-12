use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnStatus {
    pub connected: bool,
    pub local_ip: Option<String>,
    pub gateway_ip: Option<String>,
    pub latency_ms: Option<u32>,
    pub interface_name: Option<String>,
}

pub struct VpnManager {
    status: Mutex<VpnStatus>,
    config_path: Mutex<Option<String>>,
}

impl VpnManager {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(VpnStatus {
                connected: false,
                local_ip: None,
                gateway_ip: None,
                latency_ms: None,
                interface_name: None,
            }),
            config_path: Mutex::new(None),
        }
    }

    /// Initialize VPN manager with stored config path from AppConfig
    pub fn init_from_config(&self, wg_config_path: Option<String>) {
        if let Some(path) = wg_config_path {
            let p = std::path::PathBuf::from(&path);
            if p.exists() {
                *self.config_path.lock().unwrap() = Some(path);
            }
        }
    }

    /// Check if a WireGuard config is available
    pub fn has_config(&self) -> bool {
        self.config_path.lock().unwrap().is_some()
    }

    /// Run a command with admin privileges (platform-specific)
    #[cfg(target_os = "macos")]
    fn run_privileged(cmd: &str) -> Result<String, String> {
        let full_cmd = format!(
            "export PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH; {}",
            cmd
        );
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            full_cmd.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("osascript failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Err("Authorization cancelled by user".to_string());
            }
            return Err(format!("Command failed: {}", stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    #[cfg(target_os = "linux")]
    fn run_privileged(cmd: &str) -> Result<String, String> {
        let output = Command::new("pkexec")
            .args(["bash", "-c", cmd])
            .output()
            .map_err(|e| format!("pkexec failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("Command failed: {}", stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    #[cfg(target_os = "windows")]
    fn run_privileged(cmd: &str) -> Result<String, String> {
        let output = Command::new("powershell")
            .args(["-Command", &format!("Start-Process -Verb RunAs -Wait cmd -ArgumentList '/c {}'", cmd)])
            .output()
            .map_err(|e| format!("Failed to run elevated command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("Command failed: {}", stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Find the wg-quick binary path
    fn wg_quick_path() -> String {
        #[cfg(target_os = "macos")]
        {
            for path in &["/opt/homebrew/bin/wg-quick", "/usr/local/bin/wg-quick", "/usr/bin/wg-quick"] {
                if std::path::Path::new(path).exists() {
                    return path.to_string();
                }
            }
        }
        "wg-quick".to_string()
    }

    pub fn connect(&self, config_path: &str) -> Result<VpnStatus, String> {
        let path = if config_path.is_empty() {
            self.config_path
                .lock()
                .unwrap()
                .clone()
                .ok_or("No WireGuard config file configured. Import one in Settings.")?
        } else {
            config_path.to_string()
        };

        let wg_quick = Self::wg_quick_path();
        let cmd = format!("{} up '{}'", wg_quick, path);
        match Self::run_privileged(&cmd) {
            Ok(_) => {}
            Err(e) => {
                if e.contains("already exists") {
                    // Interface already up, treat as connected
                } else {
                    return Err(e);
                }
            }
        }

        *self.config_path.lock().unwrap() = Some(path);

        // Wait for interface initialization
        std::thread::sleep(std::time::Duration::from_millis(500));

        let status = self.check_status()?;
        *self.status.lock().unwrap() = status.clone();
        Ok(status)
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let config_path = self.config_path.lock().unwrap().clone();
        let path = config_path.as_deref().unwrap_or("wg0");

        let wg_quick = Self::wg_quick_path();
        let cmd = format!("{} down '{}'", wg_quick, path);
        match Self::run_privileged(&cmd) {
            Ok(_) => {}
            Err(e) => {
                if !e.contains("is not a WireGuard interface") {
                    return Err(e);
                }
            }
        }

        let mut status = self.status.lock().unwrap();
        *status = VpnStatus {
            connected: false,
            local_ip: None,
            gateway_ip: None,
            latency_ms: None,
            interface_name: None,
        };

        Ok(())
    }

    pub fn check_status(&self) -> Result<VpnStatus, String> {
        let local_ip = self.get_wireguard_ip();
        let connected = local_ip.is_some();

        let gateway_ip = local_ip.as_ref().and_then(|ip| derive_gateway(ip));

        let latency = if let Some(ref gw) = gateway_ip {
            self.ping_host(gw)
        } else {
            None
        };

        let status = VpnStatus {
            connected,
            local_ip,
            gateway_ip,
            latency_ms: latency,
            interface_name: if connected {
                Some("wg".to_string())
            } else {
                None
            },
        };

        *self.status.lock().unwrap() = status.clone();
        Ok(status)
    }

    /// Detect WireGuard interface IP dynamically
    fn get_wireguard_ip(&self) -> Option<String> {
        // Try `wg show interfaces` first
        let wg_output = Command::new("wg").arg("show").arg("interfaces").output().ok();
        if let Some(output) = wg_output {
            if output.status.success() {
                let ifaces = String::from_utf8_lossy(&output.stdout);
                if let Some(iface) = ifaces.trim().split_whitespace().next() {
                    // Get IP from this interface
                    #[cfg(unix)]
                    {
                        let ip_output = Command::new("ifconfig").arg(iface).output().ok();
                        if let Some(ip_out) = ip_output {
                            let text = String::from_utf8_lossy(&ip_out.stdout);
                            for line in text.lines() {
                                let trimmed = line.trim();
                                if trimmed.starts_with("inet ") && !trimmed.contains("127.0.0.1") {
                                    return trimmed.split_whitespace().nth(1).map(|s| s.to_string());
                                }
                            }
                        }
                    }
                    #[cfg(windows)]
                    {
                        let ip_output = Command::new("netsh")
                            .args(["interface", "ip", "show", "addresses", iface])
                            .output()
                            .ok();
                        if let Some(ip_out) = ip_output {
                            let text = String::from_utf8_lossy(&ip_out.stdout);
                            for line in text.lines() {
                                let trimmed = line.trim();
                                if trimmed.contains("IP Address") || trimmed.contains("IP 地址") {
                                    if let Some(ip) = trimmed.rsplit_once(':').or_else(|| trimmed.rsplit_once('：')) {
                                        return Some(ip.1.trim().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: scan utun/wg interfaces for private IPs
        #[cfg(unix)]
        {
            let output = Command::new("ifconfig").output().ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            let mut in_wg_iface = false;
            for line in text.lines() {
                if line.starts_with("utun") || line.starts_with("wg") {
                    in_wg_iface = true;
                } else if !line.starts_with(' ') && !line.starts_with('\t') {
                    in_wg_iface = false;
                }
                if in_wg_iface {
                    let trimmed = line.trim();
                    if trimmed.starts_with("inet ") {
                        let ip = trimmed.split_whitespace().nth(1)?;
                        if ip.starts_with("10.") || ip.starts_with("172.") || ip.starts_with("192.168.") {
                            return Some(ip.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    fn ping_host(&self, host: &str) -> Option<u32> {
        #[cfg(unix)]
        let output = Command::new("ping")
            .args(["-c", "1", "-W", "1", host])
            .output()
            .ok()?;
        #[cfg(windows)]
        let output = Command::new("ping")
            .args(["-n", "1", "-w", "1000", host])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for part in text.split_whitespace() {
            if part.starts_with("time=") {
                let ms_str = part.trim_start_matches("time=");
                return ms_str.parse::<f64>().ok().map(|v| v as u32);
            }
        }
        None
    }

    pub fn set_config_path(&self, path: String) {
        *self.config_path.lock().unwrap() = Some(path);
    }
}

/// Derive gateway IP from a local VPN IP (e.g., 10.0.0.4 -> 10.0.0.1)
fn derive_gateway(local_ip: &str) -> Option<String> {
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() == 4 {
        Some(format!("{}.{}.{}.1", parts[0], parts[1], parts[2]))
    } else {
        None
    }
}
