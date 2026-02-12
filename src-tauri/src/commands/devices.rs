use crate::config::{ConfigState, Device, save_config};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceWithStatus {
    #[serde(flatten)]
    pub device: Device,
    pub online: bool,
}

#[tauri::command]
pub async fn list_devices(
    config: State<'_, ConfigState>,
) -> Result<Vec<DeviceWithStatus>, String> {
    let cfg = config.0.lock().unwrap().clone();
    log::info!("list_devices: checking {} devices", cfg.devices.len());
    let mut results = Vec::new();

    for device in &cfg.devices {
        let check_host = device.ssh_host.as_deref().unwrap_or(&device.vpn_ip);
        let online = if device.ssh_port.is_some() || device.ssh_host.is_some() {
            // For non-VPN devices, check TCP connectivity instead of ping
            check_tcp(check_host, device.ssh_port.unwrap_or(22))
        } else {
            check_online(&device.vpn_ip)
        };
        log::info!("  {} ({}) -> online={}", device.name, check_host, online);
        results.push(DeviceWithStatus {
            device: device.clone(),
            online,
        });
    }

    Ok(results)
}

#[tauri::command]
pub async fn add_device(
    name: String,
    vpn_ip: String,
    ssh_user: String,
    rustdesk_id: Option<String>,
    ssh_host: Option<String>,
    ssh_port: Option<u16>,
    config: State<'_, ConfigState>,
) -> Result<Device, String> {
    let mut cfg = config.0.lock().unwrap();

    // Generate ID from name
    let id = name.to_lowercase().replace(' ', "-");

    // Check for duplicates
    if cfg.devices.iter().any(|d| d.id == id) {
        return Err(format!("Device '{}' already exists", id));
    }

    let device = Device {
        id,
        name,
        vpn_ip,
        ssh_user,
        rustdesk_id,
        ssh_host,
        ssh_port,
    };

    cfg.devices.push(device.clone());
    save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(device)
}

#[tauri::command]
pub async fn remove_device(
    id: String,
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    let mut cfg = config.0.lock().unwrap();
    cfg.devices.retain(|d| d.id != id);
    save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn ping_device(ip: String) -> Result<bool, String> {
    Ok(check_online(&ip))
}

#[tauri::command]
pub async fn export_config(
    config: State<'_, ConfigState>,
) -> Result<String, String> {
    let cfg = config.0.lock().unwrap().clone();
    serde_json::to_string_pretty(&cfg)
        .map_err(|e| format!("Failed to serialize config: {}", e))
}

#[tauri::command]
pub async fn import_config(
    json_str: String,
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    let new_config: crate::config::AppConfig = serde_json::from_str(&json_str)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;
    let mut cfg = config.0.lock().unwrap();
    *cfg = new_config;
    save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;
    Ok(())
}

fn check_online(ip: &str) -> bool {
    #[cfg(unix)]
    let result = std::process::Command::new("ping")
        .args(["-c", "1", "-W", "2", ip])
        .output();
    #[cfg(windows)]
    let result = std::process::Command::new("ping")
        .args(["-n", "1", "-w", "2000", ip])
        .output();
    result.map(|o| o.status.success()).unwrap_or(false)
}

fn check_tcp(host: &str, port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let addr = format!("{}:{}", host, port);
    addr.to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
        .unwrap_or(false)
}
