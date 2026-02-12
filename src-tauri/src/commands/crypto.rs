use crate::config::{self, ConfigState};
use tauri::State;

#[tauri::command]
pub async fn config_is_encrypted() -> Result<bool, String> {
    let path = config::config_path();
    if !path.exists() {
        return Ok(false);
    }
    let data = std::fs::read(&path).map_err(|e| format!("Read failed: {}", e))?;
    Ok(crate::crypto::is_encrypted(&data))
}

#[tauri::command]
pub async fn unlock_config(
    password: String,
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    let path = config::config_path();
    let data = std::fs::read(&path).map_err(|e| format!("Read failed: {}", e))?;
    let json = crate::crypto::decrypt_config(&data, &password)?;
    let app_config: config::AppConfig =
        serde_json::from_str(&json).map_err(|e| format!("Invalid config: {}", e))?;
    *config.0.lock().unwrap() = app_config;
    *config.1.lock().unwrap() = Some(password);
    Ok(())
}

#[tauri::command]
pub async fn set_config_password(
    password: String,
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    *config.1.lock().unwrap() = Some(password.clone());
    let cfg = config.0.lock().unwrap().clone();
    config::save_config_with_password(&cfg, &Some(password))
        .map_err(|e| format!("Save failed: {}", e))
}

#[tauri::command]
pub async fn remove_config_password(
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    *config.1.lock().unwrap() = None;
    let cfg = config.0.lock().unwrap().clone();
    config::save_config_with_password(&cfg, &None).map_err(|e| format!("Save failed: {}", e))
}
