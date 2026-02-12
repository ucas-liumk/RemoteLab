use crate::sshkeys::ops;

#[tauri::command]
pub async fn ssh_keys_list() -> Result<Vec<ops::SshKeyInfo>, String> {
    tokio::task::spawn_blocking(ops::list_ssh_keys)
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn ssh_key_generate(
    name: String,
    passphrase: String,
) -> Result<ops::SshKeyInfo, String> {
    tokio::task::spawn_blocking(move || ops::generate_key(&name, &passphrase))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn ssh_key_copy_to_remote(
    key_path: String,
    host: String,
    user: String,
    port: Option<u16>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || ops::copy_key_to_remote(&key_path, &host, &user, port))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}
