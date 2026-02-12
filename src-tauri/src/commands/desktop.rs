use crate::desktop::{detect, launcher, sunshine, turbovnc, VncProxy};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Connection result returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct DesktopConnection {
    /// "sunshine" or "vnc"
    pub mode: String,
    /// For sunshine: URL to web UI iframe
    /// For vnc: ws://127.0.0.1:{port} for VNC WebSocket client
    pub url: String,
    /// GPU info
    pub gpu_name: String,
    pub has_nvenc: bool,
}

fn emit_progress(app: &AppHandle, phase: &str, percent: u8, message: &str) {
    let _ = app.emit(
        "desktop-progress",
        serde_json::json!({
            "phase": phase,
            "percent": percent,
            "message": message,
        }),
    );
}

/// Smart desktop connect: auto-detect GPU → choose best streaming method
#[tauri::command]
pub async fn desktop_connect(
    app: AppHandle,
    host: String,
    user: String,
    port: Option<u16>,
    proxy: State<'_, VncProxy>,
) -> Result<DesktopConnection, String> {
    log::info!(
        "Desktop connect to {}@{}:{}, detecting GPU...",
        user,
        host,
        port.unwrap_or(22)
    );

    // Step 1: Detect GPU
    emit_progress(&app, "gpu_detect", 0, "Detecting GPU...");

    let h = host.clone();
    let u = user.clone();
    let gpu = tokio::task::spawn_blocking(move || detect::detect_remote_gpu(&h, &u, port))
        .await
        .map_err(|e| format!("GPU detect task failed: {}", e))?
        .unwrap_or_else(|e| {
            log::warn!("GPU detection failed: {}, falling back to VNC", e);
            detect::GpuInfo {
                gpu_name: "Unknown".to_string(),
                has_nvenc: false,
                has_display: true,
                driver_version: "Unknown".to_string(),
            }
        });

    log::info!(
        "GPU: {} (NVENC: {}, Display: {})",
        gpu.gpu_name,
        gpu.has_nvenc,
        gpu.has_display
    );

    let nvenc_label = if gpu.has_nvenc { "NVENC" } else { "No NVENC" };
    emit_progress(
        &app,
        "gpu_detect",
        5,
        &format!("GPU: {} ({})", gpu.gpu_name, nvenc_label),
    );

    // Step 2: Choose method based on GPU capabilities
    if gpu.has_nvenc {
        log::info!("GPU has NVENC, setting up Sunshine on {}", host);
        emit_progress(&app, "sunshine_setup", 5, "Starting Sunshine setup...");

        let h = host.clone();
        let u = user.clone();
        let app2 = app.clone();
        let sunshine_result = tokio::task::spawn_blocking(move || {
            sunshine::setup_sunshine(&h, &u, port, &app2)
        })
        .await
        .map_err(|e| format!("Sunshine task failed: {}", e))?;

        match sunshine_result {
            Ok(remote_port) => {
                emit_progress(&app, "tunnel", 95, "Creating secure tunnel...");
                let local_port = proxy.start_tunnel(&host, &user, remote_port, port)?;

                emit_progress(&app, "done", 100, "Connected!");
                return Ok(DesktopConnection {
                    mode: "sunshine".to_string(),
                    url: format!("https://127.0.0.1:{}", local_port),
                    gpu_name: gpu.gpu_name,
                    has_nvenc: true,
                });
            }
            Err(e) => {
                log::warn!("Sunshine failed: {}, falling back to VNC", e);
                emit_progress(
                    &app,
                    "vnc_fallback",
                    40,
                    &format!("Sunshine unavailable ({}), using VNC...", e),
                );
            }
        }
    }

    // VNC path: x11vnc with auto-install
    log::info!("Setting up VNC for {}", host);
    emit_progress(&app, "vnc_setup", 50, "Setting up VNC server...");

    let h = host.clone();
    let u = user.clone();
    let app3 = app.clone();
    let vnc_port = tokio::task::spawn_blocking(move || {
        turbovnc::setup_turbovnc(&h, &u, port, &app3)
    })
    .await
    .map_err(|e| format!("VNC setup task failed: {}", e))??;

    log::info!("VNC on remote port {}, starting tunnel", vnc_port);
    emit_progress(&app, "tunnel", 85, "Creating SSH tunnel...");

    proxy.start_tunnel(&host, &user, vnc_port, port)?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    emit_progress(&app, "proxy", 90, "Starting WebSocket proxy...");
    let ws_port = proxy.start_ws_proxy().await?;

    emit_progress(&app, "done", 100, "Connected!");
    log::info!("WebSocket proxy ready on ws://127.0.0.1:{}", ws_port);

    Ok(DesktopConnection {
        mode: "vnc".to_string(),
        url: format!("ws://127.0.0.1:{}", ws_port),
        gpu_name: gpu.gpu_name,
        has_nvenc: gpu.has_nvenc,
    })
}

/// Detect GPU info without connecting
#[tauri::command]
pub async fn detect_gpu(
    host: String,
    user: String,
    port: Option<u16>,
) -> Result<detect::GpuInfo, String> {
    detect::detect_remote_gpu(&host, &user, port)
}

/// Legacy VNC connect (direct, skip auto-detect)
#[tauri::command]
pub async fn vnc_connect(
    host: String,
    user: String,
    vnc_port: Option<u16>,
    port: Option<u16>,
    proxy: State<'_, VncProxy>,
) -> Result<u16, String> {
    proxy.connect(&host, &user, vnc_port, port).await
}

/// Stop desktop connection
#[tauri::command]
pub async fn vnc_disconnect(proxy: State<'_, VncProxy>) -> Result<(), String> {
    proxy.stop();
    Ok(())
}

/// Check if proxy is running
#[tauri::command]
pub async fn vnc_status(proxy: State<'_, VncProxy>) -> Result<bool, String> {
    Ok(proxy.is_running())
}

/// Fallback launchers
#[tauri::command]
pub async fn launch_rustdesk(device_id: String) -> Result<(), String> {
    launcher::launch_rustdesk_app(&device_id)
}

#[tauri::command]
pub async fn launch_moonlight(host_ip: String) -> Result<(), String> {
    launcher::launch_moonlight_app(&host_ip)
}
