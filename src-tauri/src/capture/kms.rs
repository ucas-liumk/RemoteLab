//! Linux KMS/DRM (Kernel Mode Setting / Direct Rendering Manager) 捕获实现
//!
//! 该模块通过 Linux DRM API 直接从内核帧缓冲区捕获屏幕内容，
//! 使用 DMA-BUF 实现零拷贝共享。

use super::{CaptureBackend, CaptureError, FrameRef, PixelFormat};
use libc::{c_char, c_int, c_uint, c_ulong, c_void, c_ushort, ioctl, open, O_RDWR, O_CLOEXEC, close};
use std::ffi::CString;
use std::os::fd::RawFd;
use std::ptr::null_mut;

// DRM ioctl 命令
const DRM_IOCTL_BASE: u8 = 0x64;

// DRM 模式相关常量
const DRM_MODE_CONNECTED: u32 = 1;

// DRM 对象类型
const DRM_MODE_OBJECT_CRTC: u32 = 0xcccccccc;
const DRM_MODE_OBJECT_CONNECTOR: u32 = 0xc0c0c0c0;

// DRM ioctl 号码
const DRM_IOCTL_VERSION: c_ulong = 0x80006400;
const DRM_IOCTL_GET_CAP: c_ulong = 0xc010640c;
const DRM_IOCTL_MODE_GETRESOURCES: c_ulong = 0xc0506400;
const DRM_IOCTL_MODE_GETCRTC: c_ulong = 0xc0686401;
const DRM_IOCTL_MODE_GETCONNECTOR: c_ulong = 0xc0506403;
const DRM_IOCTL_MODE_ATOMIC: c_ulong = 0xc068645c;
const DRM_IOCTL_PRIME_HANDLE_TO_FD: c_ulong = 0xc010645e;
const DRM_IOCTL_PRIME_FD_TO_HANDLE: c_ulong = 0xc010645f;

// DRM PRIME 标志
const DRM_CLOEXEC: u32 = 0x01;
const DRM_RDWR: u32 = 0x02;

// DRM 能力
const DRM_CAP_DUMB_BUFFER: u64 = 0x1;
const DRM_CAP_PRIME: u64 = 0x5;

/// DRM 版本信息结构
#[repr(C)]
struct DrmVersion {
    version_major: c_int,
    version_minor: c_int,
    version_patchlevel: c_int,
    name_len: c_ulong,
    name: *mut c_char,
    date_len: c_ulong,
    date: *mut c_char,
    desc_len: c_ulong,
    desc: *mut c_char,
}

/// DRM 模式资源结构
#[repr(C)]
struct DrmModeRes {
    fb_id_ptr: *mut u32,
    crtc_id_ptr: *mut u32,
    connector_id_ptr: *mut u32,
    encoder_id_ptr: *mut u32,
    count_fbs: c_uint,
    count_crtcs: c_uint,
    count_connectors: c_uint,
    count_encoders: c_uint,
    min_width: c_uint,
    max_width: c_uint,
    min_height: c_uint,
    max_height: c_uint,
}

/// DRM CRTC 信息结构
#[repr(C)]
struct DrmModeCrtc {
    set_connectors_ptr: *mut u32,
    connector_id: c_uint,
    crtc_id: c_uint,
    fb_id: c_uint,
    x: c_uint,
    y: c_uint,
    width: c_uint,
    height: c_uint,
    mode_valid: c_int,
    mode: DrmModeModeInfo,
    gamma_size: c_int,
}

/// DRM 模式信息结构
#[repr(C)]
#[derive(Debug, Clone)]
struct DrmModeModeInfo {
    clock: c_uint,
    hdisplay: c_ushort,
    hsync_start: c_ushort,
    hsync_end: c_ushort,
    htotal: c_ushort,
    hskew: c_ushort,
    vdisplay: c_ushort,
    vsync_start: c_ushort,
    vsync_end: c_ushort,
    vtotal: c_ushort,
    vscan: c_ushort,
    vrefresh: c_uint,
    flags: c_uint,
    type_: c_uint,
    name: [c_char; 32],
}

/// DRM 连接器信息结构
#[repr(C)]
struct DrmModeConnector {
    encoders_ptr: *mut u32,
    modes_ptr: *mut DrmModeModeInfo,
    props_ptr: *mut u32,
    prop_values_ptr: *mut u64,
    count_modes: c_uint,
    count_props: c_uint,
    count_encoders: c_uint,
    encoder_id: c_uint,
    connector_id: c_uint,
    connector_type: c_uint,
    connector_type_id: c_uint,
    connection: c_uint,
    mm_width: c_uint,
    mm_height: c_uint,
    subpixel: c_uint,
}

/// DRM PRIME 句柄到 FD 参数
#[repr(C)]
struct DrmPrimeHandle {
    handle: u32,
    flags: u32,
    fd: c_int,
}

/// DRM 获取能力参数
#[repr(C)]
struct DrmGetCap {
    capability: u64,
    value: u64,
}

/// DRM 原子提交结构
#[repr(C)]
struct DrmModeAtomic {
    flags: c_uint,
    count_objs: c_uint,
    objs_ptr: *mut u32,
    count_props_ptr: *mut u32,
    props_ptr: *mut u32,
    prop_values_ptr: *mut u64,
    reserved: u64,
    user_data: u64,
}

/// KMS/DRM 捕获实现
pub struct KmsCapture {
    /// DRM 设备文件描述符
    drm_fd: RawFd,
    /// 连接器 ID
    connector_id: u32,
    /// CRTC ID
    crtc_id: u32,
    /// 当前分辨率
    width: u32,
    height: u32,
    /// 当前帧缓冲区 ID
    fb_id: u32,
    /// 像素格式
    format: PixelFormat,
    /// 是否已初始化
    initialized: bool,
    /// 设备路径
    device_path: String,
}

impl KmsCapture {
    /// DRM 设备路径
    const DRM_DEVICE_PATH: &'static str = "/dev/dri/card0";
    const DRM_DEVICE_PATH_FALLBACK: &'static str = "/dev/dri/card1";

    /// 创建新的 KMS 捕获实例
    pub fn new() -> Result<Self, CaptureError> {
        // 查找可用的 DRM 设备
        let (drm_fd, device_path) = Self::find_drm_device()?;

        Ok(Self {
            drm_fd,
            connector_id: 0,
            crtc_id: 0,
            width: 1920,
            height: 1080,
            fb_id: 0,
            format: PixelFormat::BGRA,
            initialized: false,
            device_path,
        })
    }

    /// 检查系统是否支持 KMS
    pub fn is_available() -> bool {
        unsafe {
            let path = CString::new(Self::DRM_DEVICE_PATH).unwrap();
            let fd = open(path.as_ptr(), O_RDWR | O_CLOEXEC);
            if fd >= 0 {
                close(fd);
                true
            } else {
                // 尝试 fallback 路径
                let path2 = CString::new(Self::DRM_DEVICE_PATH_FALLBACK).unwrap();
                let fd2 = open(path2.as_ptr(), O_RDWR | O_CLOEXEC);
                if fd2 >= 0 {
                    close(fd2);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// 查找可用的 DRM 设备
    fn find_drm_device() -> Result<(RawFd, String), CaptureError> {
        unsafe {
            // 尝试主要路径
            let path = CString::new(Self::DRM_DEVICE_PATH).unwrap();
            let fd = open(path.as_ptr(), O_RDWR | O_CLOEXEC);
            if fd >= 0 {
                return Ok((fd, Self::DRM_DEVICE_PATH.to_string()));
            }

            // 尝试 fallback 路径
            let path2 = CString::new(Self::DRM_DEVICE_PATH_FALLBACK).unwrap();
            let fd2 = open(path2.as_ptr(), O_RDWR | O_CLOEXEC);
            if fd2 >= 0 {
                return Ok((fd2, Self::DRM_DEVICE_PATH_FALLBACK.to_string()));
            }

            Err(CaptureError::ResourceUnavailable(
                "无法打开 DRM 设备".to_string(),
            ))
        }
    }

    /// 检查 DRM 能力
    unsafe fn check_capability(&self, cap: u64) -> Result<u64, CaptureError> {
        let mut get_cap = DrmGetCap {
            capability: cap,
            value: 0,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_GET_CAP, &mut get_cap);
        if result < 0 {
            return Err(CaptureError::DrmError(
                format!("ioctl DRM_IOCTL_GET_CAP 失败: {}", result),
            ));
        }

        Ok(get_cap.value)
    }

    /// 获取 DRM 资源
    unsafe fn get_resources(&self) -> Result<DrmModeRes, CaptureError> {
        let mut res = DrmModeRes {
            fb_id_ptr: null_mut(),
            crtc_id_ptr: null_mut(),
            connector_id_ptr: null_mut(),
            encoder_id_ptr: null_mut(),
            count_fbs: 0,
            count_crtcs: 0,
            count_connectors: 0,
            count_encoders: 0,
            min_width: 0,
            max_width: 0,
            min_height: 0,
            max_height: 0,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res);
        if result < 0 {
            return Err(CaptureError::DrmError(
                format!("ioctl DRM_IOCTL_MODE_GETRESOURCES 失败: {}", result),
            ));
        }

        Ok(res)
    }

    /// 获取连接器信息
    unsafe fn get_connector(&self, connector_id: u32) -> Result<DrmModeConnector, CaptureError> {
        let mut conn = DrmModeConnector {
            encoders_ptr: null_mut(),
            modes_ptr: null_mut(),
            props_ptr: null_mut(),
            prop_values_ptr: null_mut(),
            count_modes: 0,
            count_props: 0,
            count_encoders: 0,
            encoder_id: 0,
            connector_id,
            connector_type: 0,
            connector_type_id: 0,
            connection: 0,
            mm_width: 0,
            mm_height: 0,
            subpixel: 0,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_MODE_GETCONNECTOR, &mut conn);
        if result < 0 {
            return Err(CaptureError::DrmError(
                format!("ioctl DRM_IOCTL_MODE_GETCONNECTOR 失败: {}", result),
            ));
        }

        Ok(conn)
    }

    /// 获取 CRTC 信息
    unsafe fn get_crtc(&self, crtc_id: u32) -> Result<DrmModeCrtc, CaptureError> {
        let mut crtc = DrmModeCrtc {
            set_connectors_ptr: null_mut(),
            connector_id: 0,
            crtc_id,
            fb_id: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            mode_valid: 0,
            mode: DrmModeModeInfo {
                clock: 0,
                hdisplay: 0,
                hsync_start: 0,
                hsync_end: 0,
                htotal: 0,
                hskew: 0,
                vdisplay: 0,
                vsync_start: 0,
                vsync_end: 0,
                vtotal: 0,
                vscan: 0,
                vrefresh: 0,
                flags: 0,
                type_: 0,
                name: [0; 32],
            },
            gamma_size: 0,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_MODE_GETCRTC, &mut crtc);
        if result < 0 {
            return Err(CaptureError::DrmError(
                format!("ioctl DRM_IOCTL_MODE_GETCRTC 失败: {}", result),
            ));
        }

        Ok(crtc)
    }

    /// 查找连接的显示器
    unsafe fn find_connected_connector(&self) -> Result<(u32, u32, u32), CaptureError> {
        let res = self.get_resources()?;

        // 分配临时缓冲区存储连接器 ID
        let mut connector_ids: Vec<u32> = vec![0; res.count_connectors as usize];
        
        let mut res_with_ptr = DrmModeRes {
            fb_id_ptr: null_mut(),
            crtc_id_ptr: null_mut(),
            connector_id_ptr: connector_ids.as_mut_ptr(),
            encoder_id_ptr: null_mut(),
            count_fbs: 0,
            count_crtcs: 0,
            count_connectors: res.count_connectors,
            count_encoders: 0,
            min_width: 0,
            max_width: 0,
            min_height: 0,
            max_height: 0,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res_with_ptr);
        if result < 0 {
            return Err(CaptureError::DrmError(
                "获取连接器列表失败".to_string(),
            ));
        }

        // 查找已连接的连接器
        for &connector_id in &connector_ids {
            if connector_id == 0 {
                continue;
            }

            let conn = self.get_connector(connector_id)?;
            if conn.connection == DRM_MODE_CONNECTED {
                // 获取当前模式的分辨率
                if conn.count_modes > 0 {
                    let mut modes: Vec<DrmModeModeInfo> = vec![
                        std::mem::zeroed();
                        conn.count_modes as usize
                    ];
                    
                    let mut conn_with_modes = DrmModeConnector {
                        encoders_ptr: null_mut(),
                        modes_ptr: modes.as_mut_ptr(),
                        props_ptr: null_mut(),
                        prop_values_ptr: null_mut(),
                        count_modes: conn.count_modes,
                        count_props: 0,
                        count_encoders: 0,
                        encoder_id: conn.encoder_id,
                        connector_id,
                        connector_type: conn.connector_type,
                        connector_type_id: conn.connector_type_id,
                        connection: conn.connection,
                        mm_width: conn.mm_width,
                        mm_height: conn.mm_height,
                        subpixel: conn.subpixel,
                    };

                    let result = ioctl(
                        self.drm_fd,
                        DRM_IOCTL_MODE_GETCONNECTOR,
                        &mut conn_with_modes,
                    );
                    if result == 0 && !modes.is_empty() {
                        let mode = &modes[0];
                        return Ok((
                            connector_id,
                            mode.hdisplay as u32,
                            mode.vdisplay as u32,
                        ));
                    }
                }

                return Ok((connector_id, 1920, 1080));
            }
        }

        Err(CaptureError::ResourceUnavailable(
            "没有找到连接的显示器".to_string(),
        ))
    }

    /// 导出 DMA-BUF
    unsafe fn export_dmabuf(&self, handle: u32) -> Result<RawFd, CaptureError> {
        let mut prime = DrmPrimeHandle {
            handle,
            flags: DRM_CLOEXEC | DRM_RDWR,
            fd: -1,
        };

        let result = ioctl(self.drm_fd, DRM_IOCTL_PRIME_HANDLE_TO_FD, &mut prime);
        if result < 0 {
            return Err(CaptureError::DrmError(
                format!("导出 DMA-BUF 失败: {}", result),
            ));
        }

        if prime.fd < 0 {
            return Err(CaptureError::DrmError(
                "导出的 DMA-BUF fd 无效".to_string(),
            ));
        }

        Ok(prime.fd)
    }

    /// 获取 DRM 版本信息
    pub fn get_version(&self) -> Result<String, CaptureError> {
        unsafe {
            let mut version: DrmVersion = std::mem::zeroed();
            let result = ioctl(self.drm_fd, DRM_IOCTL_VERSION, &mut version);
            if result < 0 {
                return Err(CaptureError::DrmError(
                    "获取 DRM 版本失败".to_string(),
                ));
            }

            Ok(format!(
                "{}.{}.{}",
                version.version_major, version.version_minor, version.version_patchlevel
            ))
        }
    }
}

impl CaptureBackend for KmsCapture {
    fn init(&mut self) -> Result<(), CaptureError> {
        if self.initialized {
            return Ok(());
        }

        unsafe {
            // 检查 DRM PRIME 支持
            let prime_cap = self.check_capability(DRM_CAP_PRIME)?;
            if prime_cap == 0 {
                return Err(CaptureError::DrmError(
                    "DRM PRIME 不支持".to_string(),
                ));
            }

            // 查找连接的显示器
            let (connector_id, width, height) = self.find_connected_connector()?;
            self.connector_id = connector_id;
            self.width = width;
            self.height = height;

            // 获取 CRTC 信息
            let res = self.get_resources()?;
            if res.count_crtcs > 0 {
                // 使用第一个 CRTC
                self.crtc_id = 1; // 简化处理
            }
        }

        self.initialized = true;
        log::info!(
            "KMS 捕获初始化成功: {}x{} (connector: {})",
            self.width,
            self.height,
            self.connector_id
        );

        Ok(())
    }

    fn capture(&mut self) -> Result<FrameRef, CaptureError> {
        if !self.initialized {
            return Err(CaptureError::InvalidCall(
                "捕获前必须先初始化".to_string(),
            ));
        }

        unsafe {
            // 获取当前 CRTC 状态
            let crtc = self.get_crtc(self.crtc_id)?;
            self.fb_id = crtc.fb_id;

            if self.fb_id == 0 {
                return Err(CaptureError::CaptureFailed(
                    "没有活动的帧缓冲区".to_string(),
                ));
            }

            // 导出 DMA-BUF
            let dmabuf_fd = self.export_dmabuf(self.fb_id)?;

            Ok(FrameRef::with_dmabuf(
                self.width,
                self.height,
                self.format,
                dmabuf_fd,
            ))
        }
    }

    fn is_zero_copy(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "KMS/DRM"
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_resolution(&mut self, width: u32, height: u32) -> Result<(), CaptureError> {
        // KMS 使用显示器的原生分辨率
        // 这里只更新内部状态，实际分辨率由显示器决定
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn cleanup(&mut self) {
        if self.drm_fd >= 0 {
            unsafe {
                close(self.drm_fd);
            }
            self.drm_fd = -1;
        }
        self.initialized = false;
        log::info!("KMS 捕获已清理");
    }
}

impl Drop for KmsCapture {
    fn drop(&mut self) {
        if self.drm_fd >= 0 {
            self.cleanup();
        }
    }
}

unsafe impl Send for KmsCapture {}
unsafe impl Sync for KmsCapture {}

#[cfg(test)]
mod kms_tests {
    use super::*;

    #[test]
    fn test_kms_is_available() {
        let _available = KmsCapture::is_available();
    }

    #[test]
    fn test_kms_capture_properties() {
        if !KmsCapture::is_available() {
            println!("跳过测试：KMS 不可用");
            return;
        }

        match KmsCapture::new() {
            Ok(capture) => {
                assert_eq!(capture.name(), "KMS/DRM");
                assert!(capture.is_zero_copy());
            }
            Err(_) => {
                println!("KMS 创建失败（在无显示设备系统上是正常的）");
            }
        }
    }
}
