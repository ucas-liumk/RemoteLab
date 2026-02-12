use std::process::Command;

pub fn launch_rustdesk_app(device_id: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Try to open RustDesk with connection ID
        let result = Command::new("open")
            .args(["-a", "RustDesk", "--args", &format!("--connect={}", device_id)])
            .spawn();

        match result {
            Ok(_) => Ok(()),
            Err(_) => {
                // Fallback: just open RustDesk
                Command::new("open")
                    .args(["-a", "RustDesk"])
                    .spawn()
                    .map_err(|e| format!("Failed to launch RustDesk: {}. Is it installed?", e))?;
                Ok(())
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("rustdesk")
            .args(["--connect", device_id])
            .spawn()
            .map_err(|e| format!("Failed to launch RustDesk: {}. Is it installed?", e))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("rustdesk.exe")
            .args(["--connect", device_id])
            .spawn()
            .map_err(|e| format!("Failed to launch RustDesk: {}. Is it installed?", e))?;
        Ok(())
    }
}

pub fn launch_moonlight_app(_host_ip: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Moonlight"])
            .spawn()
            .map_err(|e| format!("Failed to launch Moonlight: {}. Is it installed?", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Command::new("moonlight")
            .args(["stream", host_ip])
            .spawn()
            .map_err(|e| format!("Failed to launch Moonlight: {}. Is it installed?", e))?;
        Ok(())
    }
}
