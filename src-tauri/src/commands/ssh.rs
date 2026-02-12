use crate::terminal::TerminalManager;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn ssh_open(
    session_id: String,
    host: String,
    user: String,
    port: Option<u16>,
    app: AppHandle,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    manager.open_session(&session_id, &host, &user, port, app)
}

#[tauri::command]
pub async fn ssh_write(
    session_id: String,
    data: Vec<u8>,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    manager.write_session(&session_id, &data)
}

#[tauri::command]
pub async fn ssh_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    manager.resize_session(&session_id, cols, rows)
}

#[tauri::command]
pub async fn ssh_close(
    session_id: String,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    manager.close_session(&session_id)
}
