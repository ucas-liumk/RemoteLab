use crate::vpn::{VpnManager, tunnel::VpnStatus};
use tauri::State;

#[tauri::command]
pub async fn vpn_connect(
    config_path: String,
    vpn: State<'_, VpnManager>,
) -> Result<VpnStatus, String> {
    vpn.connect(&config_path)
}

#[tauri::command]
pub async fn vpn_disconnect(vpn: State<'_, VpnManager>) -> Result<(), String> {
    vpn.disconnect()
}

#[tauri::command]
pub async fn vpn_status(vpn: State<'_, VpnManager>) -> Result<VpnStatus, String> {
    vpn.check_status()
}

#[tauri::command]
pub async fn vpn_has_config(vpn: State<'_, VpnManager>) -> Result<bool, String> {
    Ok(vpn.has_config())
}

#[tauri::command]
pub async fn vpn_import_config(
    path: String,
    vpn: State<'_, VpnManager>,
) -> Result<String, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    // Validate the config
    crate::vpn::wg_config::WgConfig::parse(&content)?;

    // Copy to app config directory
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("remotelab");
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    let dest = config_dir.join("wg0.conf");
    std::fs::copy(&path, &dest)
        .map_err(|e| format!("Failed to copy config: {}", e))?;

    // Update VPN manager with new config path
    vpn.set_config_path(dest.to_string_lossy().to_string());

    Ok(dest.to_string_lossy().to_string())
}
