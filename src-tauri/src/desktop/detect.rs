use std::process::{Command, Stdio};

/// GPU capabilities detected on remote host
#[derive(Debug, Clone, serde::Serialize)]
pub struct GpuInfo {
    pub gpu_name: String,
    pub has_nvenc: bool,
    pub has_display: bool,
    pub driver_version: String,
}

/// GPUs known to have NVENC encoder
const NVENC_GPUS: &[&str] = &[
    "L40", "L4", "RTX", "GTX 16", "GTX 20",
    "T4", "T10", "Quadro P", "Quadro RTX",
    "Tesla T", "A10", "A16", "A2",
];

/// GPUs known to NOT have NVENC (compute-only)
const NO_NVENC_GPUS: &[&str] = &[
    "A100", "A800", "H100", "H200", "H800",
    "V100", "MI", "Instinct",
];

impl GpuInfo {
    pub fn has_nvenc_support(&self) -> bool {
        self.has_nvenc
    }
}

/// Detect GPU capabilities on remote host via SSH
pub fn detect_remote_gpu(host: &str, user: &str, port: Option<u16>) -> Result<GpuInfo, String> {
    log::info!("Detecting GPU on {}@{}:{}", user, host, port.unwrap_or(22));

    let script = r#"
# GPU info — check nvidia-smi exit code first
if nvidia-smi --query-gpu=name --format=csv,noheader > /tmp/.remotelab_gpu 2>/dev/null; then
    GPU_NAME=$(head -1 /tmp/.remotelab_gpu | xargs)
    DRIVER=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 | xargs)
else
    GPU_NAME=""
    DRIVER=""
fi
rm -f /tmp/.remotelab_gpu 2>/dev/null

if [ -z "$GPU_NAME" ]; then
    echo "GPU:NONE"
    echo "DRIVER:NONE"
else
    echo "GPU:$GPU_NAME"
    echo "DRIVER:$DRIVER"
fi

# Check NVENC — only if nvidia-smi works
if [ -n "$GPU_NAME" ]; then
    if nvidia-smi -q 2>/dev/null | grep -qi "Encoder\|NVENC"; then
        echo "NVENC:maybe"
    else
        echo "NVENC:no"
    fi
else
    echo "NVENC:no"
fi

# Check for display (X11)
HAS_DISPLAY=no
for d in /tmp/.X11-unix/X*; do
    if [ -e "$d" ]; then
        HAS_DISPLAY=yes
        break
    fi
done
echo "DISPLAY:$HAS_DISPLAY"
"#;

    let mut ssh_args = vec![
        "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(), "ConnectTimeout=10".to_string(),
    ];
    if let Some(p) = port {
        ssh_args.push("-p".to_string());
        ssh_args.push(p.to_string());
    }
    ssh_args.push(format!("{}@{}", user, host));
    ssh_args.push("bash".to_string());

    #[cfg(unix)]
    let ssh_bin = "/usr/bin/ssh";
    #[cfg(windows)]
    let ssh_bin = "ssh";
    let output = Command::new(ssh_bin)
        .args(&ssh_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(script.as_bytes()).ok();
            }
            child.wait_with_output()
        })
        .map_err(|e| format!("SSH failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    log::info!("GPU detect output: {}", stdout);

    let mut gpu_name = "Unknown".to_string();
    let mut driver_version = "Unknown".to_string();
    let mut nvenc_hint = false;
    let mut has_display = false;

    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix("GPU:") {
            gpu_name = name.trim().to_string();
        } else if let Some(drv) = line.strip_prefix("DRIVER:") {
            driver_version = drv.trim().to_string();
        } else if line.starts_with("NVENC:maybe") {
            nvenc_hint = true;
        } else if line.starts_with("DISPLAY:yes") {
            has_display = true;
        }
    }

    // Determine NVENC support from GPU name
    let has_nvenc = if gpu_name == "NONE" || gpu_name == "Unknown" {
        false
    } else if NO_NVENC_GPUS.iter().any(|g| gpu_name.contains(g)) {
        false
    } else if NVENC_GPUS.iter().any(|g| gpu_name.contains(g)) {
        true
    } else {
        nvenc_hint
    };

    let info = GpuInfo {
        gpu_name,
        has_nvenc,
        has_display,
        driver_version,
    };

    log::info!("GPU detected: {:?}", info);
    Ok(info)
}
