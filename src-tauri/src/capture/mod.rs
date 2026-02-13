//! RemoteLab Ultra 零拷贝屏幕捕获模块
//!
//! 支持三种捕获后端：
//! - NvFBC: NVIDIA GPU 零拷贝捕获 (CUDA 设备指针)
//! - KMS: Linux DRM/KMS 捕获 (DMA-BUF 文件描述符)
//! - X11: 通用回退实现

pub mod kms;
pub mod nvfbcs;
pub mod tests;
pub mod x11;

use std::os::fd::RawFd;
use std::time::Instant;
use thiserror::Error;

/// 像素格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// BGRA 32位格式
    BGRA,
    /// RGBA 32位格式
    RGBA,
    /// NV12 YUV格式
    NV12,
    /// YUV420 YUV格式
    YUV420,
}

impl PixelFormat {
    /// 获取每个像素占用的字节数
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::BGRA | PixelFormat::RGBA => 4,
            PixelFormat::NV12 => 1,
            PixelFormat::YUV420 => 1,
        }
    }

    /// 获取格式名称
    pub fn name(&self) -> &'static str {
        match self {
            PixelFormat::BGRA => "BGRA",
            PixelFormat::RGBA => "RGBA",
            PixelFormat::NV12 => "NV12",
            PixelFormat::YUV420 => "YUV420",
        }
    }
}

/// 帧引用结构 - 零拷贝捕获的核心
#[derive(Debug)]
pub struct FrameRef {
    /// CUDA 设备指针 (NvFBC 使用)
    pub gpu_ptr: Option<usize>,
    /// DMA-BUF 文件描述符 (KMS 使用)
    pub dmabuf_fd: Option<RawFd>,
    /// 帧宽度 (像素)
    pub width: u32,
    /// 帧高度 (像素)
    pub height: u32,
    /// 像素格式
    pub format: PixelFormat,
    /// 捕获时间戳
    pub timestamp: Instant,
    /// 帧数据在 GPU 内存中的大小 (字节)
    pub data_size: usize,
}

impl FrameRef {
    /// 创建新的 FrameRef
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let data_size = Self::calculate_data_size(width, height, format);
        Self {
            gpu_ptr: None,
            dmabuf_fd: None,
            width,
            height,
            format,
            timestamp: Instant::now(),
            data_size,
        }
    }

    /// 创建带有 CUDA 指针的 FrameRef
    pub fn with_gpu_ptr(width: u32, height: u32, format: PixelFormat, gpu_ptr: usize) -> Self {
        let data_size = Self::calculate_data_size(width, height, format);
        Self {
            gpu_ptr: Some(gpu_ptr),
            dmabuf_fd: None,
            width,
            height,
            format,
            timestamp: Instant::now(),
            data_size,
        }
    }

    /// 创建带有 DMA-BUF fd 的 FrameRef
    pub fn with_dmabuf(width: u32, height: u32, format: PixelFormat, fd: RawFd) -> Self {
        let data_size = Self::calculate_data_size(width, height, format);
        Self {
            gpu_ptr: None,
            dmabuf_fd: Some(fd),
            width,
            height,
            format,
            timestamp: Instant::now(),
            data_size,
        }
    }

    /// 计算帧数据大小
    fn calculate_data_size(width: u32, height: u32, format: PixelFormat) -> usize {
        match format {
            PixelFormat::BGRA | PixelFormat::RGBA => {
                (width * height * 4) as usize
            }
            PixelFormat::NV12 => {
                (width * height * 3 / 2) as usize
            }
            PixelFormat::YUV420 => {
                (width * height * 3 / 2) as usize
            }
        }
    }

    /// 检查是否为有效的零拷贝帧
    pub fn is_valid(&self) -> bool {
        self.gpu_ptr.is_some() || self.dmabuf_fd.is_some()
    }

    /// 获取帧数据大小
    pub fn data_size(&self) -> usize {
        self.data_size
    }

    /// 获取时间戳
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }
}

impl Drop for FrameRef {
    fn drop(&mut self) {
        if let Some(fd) = self.dmabuf_fd {
            unsafe {
                libc::close(fd);
            }
        }
    }
}

/// 捕获错误类型
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("初始化失败: {0}")]
    InitFailed(String),

    #[error("捕获失败: {0}")]
    CaptureFailed(String),

    #[error("CUDA 错误: {0}")]
    CudaError(String),

    #[error("DRM/KMS 错误: {0}")]
    DrmError(String),

    #[error("X11 错误: {0}")]
    X11Error(String),

    #[error("不支持的格式: {0}")]
    UnsupportedFormat(String),

    #[error("资源不可用: {0}")]
    ResourceUnavailable(String),

    #[error("无效调用: {0}")]
    InvalidCall(String),

    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),

    #[error("未知错误: {0}")]
    Unknown(String),
}

/// 捕获后端 trait
pub trait CaptureBackend: Send + Sync {
    /// 初始化捕获
    fn init(&mut self) -> Result<(), CaptureError>;

    /// 捕获一帧，返回 FrameRef（零拷贝）
    fn capture(&mut self) -> Result<FrameRef, CaptureError>;

    /// 检查是否支持零拷贝
    fn is_zero_copy(&self) -> bool;

    /// 获取捕获方式名称
    fn name(&self) -> &'static str;

    /// 获取当前分辨率
    fn resolution(&self) -> (u32, u32);

    /// 设置目标分辨率
    fn set_resolution(&mut self, width: u32, height: u32) -> Result<(), CaptureError>;

    /// 清理资源
    fn cleanup(&mut self);
}

/// 自动检测并返回最佳可用的捕获后端
pub fn detect_best_capture() -> Result<Box<dyn CaptureBackend>, CaptureError> {
    log::info!("尝试初始化 NvFBC 捕获...");
    match nvfbcs::NvFBCCapture::new() {
        Ok(mut capture) => {
            match capture.init() {
                Ok(()) => {
                    log::info!("NvFBC 捕获初始化成功");
                    return Ok(Box::new(capture));
                }
                Err(e) => {
                    log::warn!("NvFBC 初始化失败: {}", e);
                }
            }
        }
        Err(e) => {
            log::warn!("NvFBC 创建失败: {}", e);
        }
    }

    log::info!("尝试初始化 KMS 捕获...");
    match kms::KmsCapture::new() {
        Ok(mut capture) => {
            match capture.init() {
                Ok(()) => {
                    log::info!("KMS 捕获初始化成功");
                    return Ok(Box::new(capture));
                }
                Err(e) => {
                    log::warn!("KMS 初始化失败: {}", e);
                }
            }
        }
        Err(e) => {
            log::warn!("KMS 创建失败: {}", e);
        }
    }

    log::info!("尝试初始化 X11 捕获...");
    match x11::X11Capture::new() {
        Ok(mut capture) => {
            match capture.init() {
                Ok(()) => {
                    log::info!("X11 捕获初始化成功");
                    Ok(Box::new(capture))
                }
                Err(e) => {
                    log::error!("X11 初始化失败: {}", e);
                    Err(CaptureError::InitFailed(
                        "没有可用的捕获后端".to_string(),
                    ))
                }
            }
        }
        Err(e) => {
            log::error!("X11 创建失败: {}", e);
            Err(CaptureError::InitFailed(
                "没有可用的捕获后端".to_string(),
            ))
        }
    }
}

/// 检查系统是否支持 NvFBC
pub fn is_nvfbcs_available() -> bool {
    nvfbcs::NvFBCCapture::is_available()
}

/// 检查系统是否支持 KMS
pub fn is_kms_available() -> bool {
    kms::KmsCapture::is_available()
}

/// 检查系统是否支持 X11
pub fn is_x11_available() -> bool {
    x11::X11Capture::is_available()
}

/// 获取所有可用捕获后端列表
pub fn list_available_backends() -> Vec<&'static str> {
    let mut backends = Vec::new();
    
    if is_nvfbcs_available() {
        backends.push("NvFBC");
    }
    if is_kms_available() {
        backends.push("KMS/DRM");
    }
    if is_x11_available() {
        backends.push("X11");
    }
    
    backends
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_pixel_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::BGRA.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::RGBA.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::NV12.bytes_per_pixel(), 1);
        assert_eq!(PixelFormat::YUV420.bytes_per_pixel(), 1);
    }

    #[test]
    fn test_pixel_format_name() {
        assert_eq!(PixelFormat::BGRA.name(), "BGRA");
        assert_eq!(PixelFormat::RGBA.name(), "RGBA");
        assert_eq!(PixelFormat::NV12.name(), "NV12");
        assert_eq!(PixelFormat::YUV420.name(), "YUV420");
    }

    #[test]
    fn test_frame_ref_new() {
        let frame = FrameRef::new(1920, 1080, PixelFormat::BGRA);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.format, PixelFormat::BGRA);
        assert!(frame.gpu_ptr.is_none());
        assert!(frame.dmabuf_fd.is_none());
        assert_eq!(frame.data_size, 1920 * 1080 * 4);
    }

    #[test]
    fn test_frame_ref_with_gpu_ptr() {
        let frame = FrameRef::with_gpu_ptr(1920, 1080, PixelFormat::RGBA, 0xdeadbeef);
        assert_eq!(frame.gpu_ptr, Some(0xdeadbeef));
        assert!(frame.dmabuf_fd.is_none());
        assert!(frame.is_valid());
    }

    #[test]
    fn test_frame_ref_with_dmabuf() {
        let frame = FrameRef::with_dmabuf(1920, 1080, PixelFormat::NV12, 42);
        assert!(frame.gpu_ptr.is_none());
        assert_eq!(frame.dmabuf_fd, Some(42));
        assert!(frame.is_valid());
    }

    #[test]
    fn test_frame_ref_nv12_size() {
        let frame = FrameRef::new(1920, 1080, PixelFormat::NV12);
        let expected_size = (1920 * 1080 * 3 / 2) as usize;
        assert_eq!(frame.data_size, expected_size);
    }
}
