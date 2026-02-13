//! 平台适配模块 - Platform Agent
//!
//! 职责：
//! 1. GPU 检测与初始化
//! 2. 驱动程序管理
//! 3. 平台特定功能抽象

use thiserror::Error;

/// 平台错误
#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("GPU not found")]
    GpuNotFound,
    
    #[error("GPU initialization failed: {0}")]
    GpuInitFailed(String),
    
    #[error("Driver not supported: {0}")]
    DriverNotSupported(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Platform not supported: {0}")]
    PlatformNotSupported(String),
}

/// GPU 厂商
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    NVIDIA,
    AMD,
    Intel,
    Apple,
    Unknown,
}

/// GPU 信息
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU 厂商
    pub vendor: GpuVendor,
    /// GPU 型号名称
    pub name: String,
    /// 显存大小 (MB)
    pub vram_mb: u64,
    /// 是否支持硬件编码
    pub supports_hardware_encode: bool,
    /// 是否支持硬件解码
    pub supports_hardware_decode: bool,
    /// 支持的编码格式
    pub supported_codecs: Vec<String>,
    /// 驱动版本
    pub driver_version: String,
    /// CUDA 版本 (NVIDIA)
    pub cuda_version: Option<String>,
    /// 是否支持 NvFBC
    pub supports_nvfbc: bool,
    /// 是否支持 NVENC
    pub supports_nvenc: bool,
}

/// 平台功能
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// 操作系统类型
    pub os_type: OsType,
    /// 可用的 GPU 列表
    pub gpus: Vec<GpuInfo>,
    /// 是否支持 CUDA
    pub supports_cuda: bool,
    /// 是否支持 DXGI
    pub supports_dxgi: bool,
    /// 是否支持 DMA-BUF
    pub supports_dmabuf: bool,
    /// 是否支持 ScreenCaptureKit
    pub supports_screencapturekit: bool,
}

/// 操作系统类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsType {
    Windows,
    Linux,
    MacOS,
}

/// GPU 管理器
pub struct GpuManager {
    gpus: Vec<GpuInfo>,
    selected_gpu: Option<usize>,
}

impl GpuManager {
    pub fn new() -> Result<Self, PlatformError> {
        let gpus = Self::detect_gpus()?;
        Ok(Self {
            gpus,
            selected_gpu: None,
        })
    }
    
    /// 检测系统中的 GPU
    fn detect_gpus() -> Result<Vec<GpuInfo>, PlatformError> {
        let mut gpus = Vec::new();
        
        #[cfg(target_os = "windows")]
        {
            // TODO: 使用 DXGI 或 NVAPI 检测 GPU
        }
        
        #[cfg(target_os = "linux")]
        {
            // TODO: 使用 PCI 或 drm 检测 GPU
        }
        
        #[cfg(target_os = "macos")]
        {
            // TODO: 使用 Metal 检测 GPU
        }
        
        // 如果没有检测到 GPU，返回空列表
        Ok(gpus)
    }
    
    /// 获取所有 GPU
    pub fn list_gpus(&self) -> &[GpuInfo] {
        &self.gpus
    }
    
    /// 选择最佳 GPU
    pub fn select_best_gpu(&mut self) -> Option<&GpuInfo> {
        // 优先选择支持硬件编码的 NVIDIA GPU
        let best_idx = self.gpus.iter().enumerate()
            .filter(|(_, g)| g.supports_hardware_encode)
            .max_by_key(|(_, g)| match g.vendor {
                GpuVendor::NVIDIA if g.supports_nvenc => 100,
                GpuVendor::AMD => 50,
                GpuVendor::Intel => 30,
                _ => 0,
            })
            .map(|(i, _)| i);
        
        self.selected_gpu = best_idx;
        best_idx.map(|i| &self.gpus[i])
    }
    
    /// 获取选中的 GPU
    pub fn get_selected_gpu(&self) -> Option<&GpuInfo> {
        self.selected_gpu.map(|i| &self.gpus[i])
    }
    
    /// 检查是否支持 NvFBC
    pub fn supports_nvfbc(&self) -> bool {
        self.get_selected_gpu()
            .map(|g| g.supports_nvfbc)
            .unwrap_or(false)
    }
    
    /// 检查是否支持 NVENC
    pub fn supports_nvenc(&self) -> bool {
        self.get_selected_gpu()
            .map(|g| g.supports_nvenc)
            .unwrap_or(false)
    }
}

impl Default for GpuManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            gpus: vec![],
            selected_gpu: None,
        })
    }
}

/// 驱动管理器
pub struct DriverManager {
    // TODO: 驱动状态
}

impl DriverManager {
    pub fn new() -> Self {
        Self {}
    }
    
    /// 检查 NVIDIA 驱动状态
    pub fn check_nvidia_driver(&self) -> Result<(), PlatformError> {
        // TODO: 检查 NVML 或 nvidia-smi
        Ok(())
    }
    
    /// 检查 CUDA 可用性
    pub fn check_cuda(&self) -> Result<String, PlatformError> {
        // TODO: 返回 CUDA 版本
        Ok("12.0".to_string())
    }
    
    /// 加载 NvFBC 库
    pub fn load_nvfbc(&self) -> Result<(), PlatformError> {
        // TODO: 动态加载 NvFBC 库
        Ok(())
    }
    
    /// 加载 NVENC 库
    pub fn load_nvenc(&self) -> Result<(), PlatformError> {
        // TODO: 动态加载 NVENC 库
        Ok(())
    }
}

impl Default for DriverManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 获取平台功能
pub fn get_platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        os_type: get_os_type(),
        gpus: vec![],
        supports_cuda: cfg!(feature = "cuda"),
        supports_dxgi: cfg!(target_os = "windows"),
        supports_dmabuf: cfg!(target_os = "linux"),
        supports_screencapturekit: cfg!(target_os = "macos"),
    }
}

/// 获取操作系统类型
fn get_os_type() -> OsType {
    #[cfg(target_os = "windows")]
    return OsType::Windows;
    
    #[cfg(target_os = "linux")]
    return OsType::Linux;
    
    #[cfg(target_os = "macos")]
    return OsType::MacOS;
}

/// 平台初始化
pub fn initialize_platform() -> Result<PlatformCapabilities, PlatformError> {
    let mut caps = get_platform_capabilities();
    
    // 检测 GPU
    let gpu_manager = GpuManager::new()?;
    caps.gpus = gpu_manager.list_gpus().to_vec();
    
    // 检查 CUDA
    #[cfg(feature = "cuda")]
    {
        caps.supports_cuda = check_cuda_available();
    }
    
    Ok(caps)
}

/// 检查 CUDA 是否可用
#[cfg(feature = "cuda")]
fn check_cuda_available() -> bool {
    // TODO: 实际 CUDA 检测
    false
}

#[cfg(not(feature = "cuda"))]
fn check_cuda_available() -> bool {
    false
}

/// 创建 GPU 管理器
pub fn create_gpu_manager() -> Result<GpuManager, PlatformError> {
    GpuManager::new()
}

/// 创建驱动管理器
pub fn create_driver_manager() -> DriverManager {
    DriverManager::new()
}
