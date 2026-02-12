use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, Manager,
};

pub fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let vpn_toggle = MenuItem::with_id(app, "vpn_toggle", "VPN: Disconnected", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit RemoteLab", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&vpn_toggle, &separator, &show, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("RemoteLab - Disconnected")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "vpn_toggle" => {
                log::info!("VPN toggle clicked");
                // TODO: toggle VPN connection
            }
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    window.show().ok();
                    window.set_focus().ok();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        window.hide().ok();
                    } else {
                        window.show().ok();
                        window.set_focus().ok();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
