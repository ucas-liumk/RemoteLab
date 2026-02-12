use tauri::AppHandle;

use crate::filetransfer::ops::{self, RemoteFile};

#[tauri::command]
pub async fn sftp_list(
    host: String,
    user: String,
    port: Option<u16>,
    path: String,
) -> Result<Vec<RemoteFile>, String> {
    tokio::task::spawn_blocking(move || ops::list_remote_dir(&host, &user, port, &path))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn sftp_upload(
    host: String,
    user: String,
    port: Option<u16>,
    local_path: String,
    remote_path: String,
    app: AppHandle,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        ops::upload_file(&host, &user, port, &local_path, &remote_path, &app)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn sftp_download(
    host: String,
    user: String,
    port: Option<u16>,
    remote_path: String,
    local_path: String,
    app: AppHandle,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        ops::download_file(&host, &user, port, &remote_path, &local_path, &app)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn sftp_mkdir(
    host: String,
    user: String,
    port: Option<u16>,
    path: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || ops::make_remote_dir(&host, &user, port, &path))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn sftp_delete(
    host: String,
    user: String,
    port: Option<u16>,
    path: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || ops::delete_remote(&host, &user, port, &path))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}
