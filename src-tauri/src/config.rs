use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{App, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub vpn_ip: String,
    pub ssh_user: String,
    pub rustdesk_id: Option<String>,
    /// Override SSH host (if different from vpn_ip, e.g. public hostname)
    #[serde(default)]
    pub ssh_host: Option<String>,
    /// Override SSH port (default 22)
    #[serde(default)]
    pub ssh_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub devices: Vec<Device>,
    pub wg_config_path: Option<String>,
    pub default_ssh_user: String,
    pub rustdesk_server: Option<String>,
    pub rustdesk_key: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            devices: vec![],
            wg_config_path: None,
            default_ssh_user: "root".to_string(),
            rustdesk_server: None,
            rustdesk_key: None,
        }
    }
}

pub struct ConfigState(pub Mutex<AppConfig>, pub Mutex<Option<String>>);

pub fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("remotelab");
    fs::create_dir_all(&config_dir).ok();
    config_dir.join("config.json")
}

pub fn init_config(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    let config = if path.exists() {
        let data = fs::read(&path)?;
        if crate::crypto::is_encrypted(&data) {
            // Config is encrypted — start with empty config, frontend will prompt for password
            AppConfig::default()
        } else {
            let text = String::from_utf8(data)?;
            serde_json::from_str(&text).unwrap_or_default()
        }
    } else {
        let config = AppConfig::default();
        let data = serde_json::to_string_pretty(&config)?;
        fs::write(&path, data)?;
        config
    };

    app.manage(ConfigState(Mutex::new(config), Mutex::new(None)));
    Ok(())
}

pub fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    let data = serde_json::to_string_pretty(config)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn save_config_with_password(
    config: &AppConfig,
    password: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    let json = serde_json::to_string_pretty(config)?;
    if let Some(pw) = password {
        let encrypted = crate::crypto::encrypt_config(&json, pw)?;
        fs::write(&path, encrypted)?;
    } else {
        fs::write(&path, json)?;
    }
    Ok(())
}
