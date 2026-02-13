mod commands;
mod config;
mod crypto;
mod tray;
mod vpn;
mod terminal;
mod desktop;
mod filetransfer;
mod sshkeys;
mod transport;
mod quality;
mod capture;
mod encoder;
mod platform;

use tauri::Manager;

/// 初始化流媒体管道
async fn init_streaming_pipeline(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // 检测 GPU 并选择最佳配置
    let gpu_manager = platform::create_gpu_manager()?;
    let gpu = gpu_manager.get_selected_gpu();
    
    if let Some(gpu) = gpu {
        log::info!("Using GPU: {} ({:?})", gpu.name, gpu.vendor);
        log::info!("  - NVENC: {}", gpu.supports_nvenc);
        log::info!("  - NvFBC: {}", gpu.supports_nvfbc);
    } else {
        log::warn!("No GPU detected, falling back to software encoding");
    }
    
    // 初始化各模块...
    // TODO: 启动流媒体服务
    
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            config::init_config(app)?;
            tray::setup_tray(app)?;
            app.manage(terminal::TerminalManager::new());
            let vpn = vpn::VpnManager::new();
            {
                let cfg = app.state::<config::ConfigState>();
                let config = cfg.0.lock().unwrap();
                vpn.init_from_config(config.wg_config_path.clone());
            }
            app.manage(vpn);
            app.manage(desktop::VncProxy::new());
            
            // 初始化流媒体管道
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = init_streaming_pipeline(&app_handle).await {
                    log::error!("Failed to initialize streaming pipeline: {}", e);
                }
            });

            log::info!("RemoteLab Ultra started");
            log::info!("Target latency: < 16ms");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // VPN
            commands::vpn::vpn_connect,
            commands::vpn::vpn_disconnect,
            commands::vpn::vpn_status,
            commands::vpn::vpn_import_config,
            commands::vpn::vpn_has_config,
            // SSH
            commands::ssh::ssh_open,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_close,
            // Devices
            commands::devices::list_devices,
            commands::devices::add_device,
            commands::devices::remove_device,
            commands::devices::ping_device,
            commands::devices::export_config,
            commands::devices::import_config,
            // Desktop (smart auto-detect + embedded)
            commands::desktop::desktop_connect,
            commands::desktop::detect_gpu,
            commands::desktop::vnc_connect,
            commands::desktop::vnc_disconnect,
            commands::desktop::vnc_status,
            commands::desktop::launch_rustdesk,
            commands::desktop::launch_moonlight,
            // File transfer
            commands::files::sftp_list,
            commands::files::sftp_upload,
            commands::files::sftp_download,
            commands::files::sftp_mkdir,
            commands::files::sftp_delete,
            // SSH Key Management
            commands::sshkeys::ssh_keys_list,
            commands::sshkeys::ssh_key_generate,
            commands::sshkeys::ssh_key_copy_to_remote,
            // Config encryption
            commands::crypto::config_is_encrypted,
            commands::crypto::unlock_config,
            commands::crypto::set_config_password,
            commands::crypto::remove_config_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RemoteLab");
}
