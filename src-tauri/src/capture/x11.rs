//! X11 屏幕捕获实现（回退方案）
//!
//! 当 NvFBC 和 KMS 都不可用时，使用 X11 进行屏幕捕获。
//! 注意：X11 捕获需要数据复制，不支持真正的零拷贝。
//!
//! # 要求
//! - X11 显示服务器
//! - libX11
//!
//! # 性能考虑
//! X11 捕获需要通过 XGetImage 将数据从 X 服务器复制到应用程序内存，
//! 因此性能低于 NvFBC 和 KMS。建议仅在无法使用其他后端时使用。

use super::{CaptureBackend, CaptureError, FrameRef, PixelFormat};
use libc::{c_char, c_int, c_uint, c_ulong, c_void, dlopen, dlsym, RTLD_NOW};
use std::ffi::CString;
use std::ptr::null_mut;

// X11 类型定义
type Display = c_void;
type Window = c_ulong;
type Drawable = c_ulong;
type XImage = c_void;
type Visual = c_void;

// X11 常量
const None: c_ulong = 0;
const AllPlanes: c_ulong = !0;
const ZPixmap: c_int = 2;

// X11 函数类型
type XOpenDisplayFn = unsafe extern "C" fn(*const c_char) -> *mut Display;
type XCloseDisplayFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XDefaultRootWindowFn = unsafe extern "C" fn(*mut Display) -> Window;
type XDisplayWidthFn = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
type XDisplayHeightFn = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
type XDefaultScreenFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XGetImageFn = unsafe extern "C" fn(
    *mut Display,
    Drawable,
    c_int,
    c_int,
    c_uint,
    c_uint,
    c_ulong,
    c_int,
) -> *mut XImage;
type XDestroyImageFn = unsafe extern "C" fn(*mut XImage) -> c_int;
type XGetErrorTextFn = unsafe extern "C" fn(*mut Display, c_int, *mut c_char, c_int);

/// XImage 结构 (简化版)
#[repr(C)]
struct XImageStruct {
    width: c_int,
    height: c_int,
    xoffset: c_int,
    format: c_int,
    data: *mut c_char,
    byte_order: c_int,
    bitmap_unit: c_int,
    bitmap_bit_order: c_int,
    bitmap_pad: c_int,
    depth: c_int,
    bytes_per_line: c_int,
    bits_per_pixel: c_int,
    red_mask: c_ulong,
    green_mask: c_ulong,
    blue_mask: c_ulong,
}

/// X11 捕获实现
pub struct X11Capture {
    /// X11 库句柄
    x11_lib: *mut c_void,
    /// X 显示连接
    display: *mut Display,
    /// 根窗口
    root_window: Window,
    /// 默认屏幕
    screen: c_int,
    /// 当前分辨率
    width: u32,
    height: u32,
    /// 像素格式
    format: PixelFormat,
    /// 是否已初始化
    initialized: bool,
    /// 函数指针
    funcs: X11Functions,
    /// 系统内存缓冲区 (用于非零拷贝回退)
    frame_buffer: Option<Vec<u8>>,
}

/// X11 函数指针集合
struct X11Functions {
    open_display: Option<XOpenDisplayFn>,
    close_display: Option<XCloseDisplayFn>,
    default_root_window: Option<XDefaultRootWindowFn>,
    display_width: Option<XDisplayWidthFn>,
    display_height: Option<XDisplayHeightFn>,
    default_screen: Option<XDefaultScreenFn>,
    get_image: Option<XGetImageFn>,
    destroy_image: Option<XDestroyImageFn>,
    get_error_text: Option<XGetErrorTextFn>,
}

impl X11Capture {
    /// X11 库名称
    const X11_LIB_NAME: &'static str = "libX11.so.6";

    /// 创建新的 X11 捕获实例
    pub fn new() -> Result<Self, CaptureError> {
        if !Self::is_available() {
            return Err(CaptureError::ResourceUnavailable(
                "X11 库未找到".to_string(),
            ));
        }

        // 加载 X11 库
        let x11_lib = unsafe {
            let lib_name = CString::new(Self::X11_LIB_NAME).unwrap();
            dlopen(lib_name.as_ptr(), RTLD_NOW)
        };

        if x11_lib.is_null() {
            return Err(CaptureError::ResourceUnavailable(
                "无法加载 X11 库".to_string(),
            ));
        }

        // 加载 X11 函数
        let funcs = unsafe { Self::load_x11_functions(x11_lib) }?;

        Ok(Self {
            x11_lib,
            display: null_mut(),
            root_window: 0,
            screen: 0,
            width: 1920,
            height: 1080,
            format: PixelFormat::BGRA,
            initialized: false,
            funcs,
            frame_buffer: None,
        })
    }

    /// 检查系统是否支持 X11
    pub fn is_available() -> bool {
        unsafe {
            // 检查环境变量
            let display_env = std::env::var("DISPLAY");
            if display_env.is_err() || display_env.unwrap().is_empty() {
                return false;
            }

            // 检查库是否存在
            let lib_name = CString::new(Self::X11_LIB_NAME).unwrap();
            let handle = dlopen(lib_name.as_ptr(), libc::RTLD_LAZY);
            if !handle.is_null() {
                libc::dlclose(handle);
                true
            } else {
                false
            }
        }
    }

    /// 加载 X11 函数指针
    unsafe fn load_x11_functions(lib: *mut c_void) -> Result<X11Functions, CaptureError> {
        let open_display_sym = CString::new("XOpenDisplay").unwrap();
        let close_display_sym = CString::new("XCloseDisplay").unwrap();
        let default_root_sym = CString::new("XDefaultRootWindow").unwrap();
        let display_width_sym = CString::new("XDisplayWidth").unwrap();
        let display_height_sym = CString::new("XDisplayHeight").unwrap();
        let default_screen_sym = CString::new("XDefaultScreen").unwrap();
        let get_image_sym = CString::new("XGetImage").unwrap();
        let destroy_image_sym = CString::new("XDestroyImage").unwrap();
        let get_error_text_sym = CString::new("XGetErrorText").unwrap();

        Ok(X11Functions {
            open_display: std::mem::transmute(dlsym(lib, open_display_sym.as_ptr())),
            close_display: std::mem::transmute(dlsym(lib, close_display_sym.as_ptr())),
            default_root_window: std::mem::transmute(dlsym(lib, default_root_sym.as_ptr())),
            display_width: std::mem::transmute(dlsym(lib, display_width_sym.as_ptr())),
            display_height: std::mem::transmute(dlsym(lib, display_height_sym.as_ptr())),
            default_screen: std::mem::transmute(dlsym(lib, default_screen_sym.as_ptr())),
            get_image: std::mem::transmute(dlsym(lib, get_image_sym.as_ptr())),
            destroy_image: std::mem::transmute(dlsym(lib, destroy_image_sym.as_ptr())),
            get_error_text: std::mem::transmute(dlsym(lib, get_error_text_sym.as_ptr())),
        })
    }

    /// 获取 X11 错误信息
    unsafe fn get_error_string(&self, error_code: c_int) -> String {
        let mut buffer: [c_char; 256] = [0; 256];
        if let Some(get_error_text) = self.funcs.get_error_text {
            get_error_text(self.display, error_code, buffer.as_mut_ptr(), 256);
            let c_str = CString::from_raw(buffer.as_mut_ptr());
            c_str.to_string_lossy().to_string()
        } else {
            format!("X11 错误码: {}", error_code)
        }
    }

    /// 分配帧缓冲区
    fn allocate_buffer(&mut self) {
        let size = (self.width * self.height * 4) as usize;
        self.frame_buffer = Some(vec![0u8; size]);
    }

    /// 从 XImage 复制数据到缓冲区
    unsafe fn copy_from_ximage(&mut self, ximage: *mut XImage) -> Result<(), CaptureError> {
        if ximage.is_null() {
            return Err(CaptureError::CaptureFailed(
                "XGetImage 返回空指针".to_string(),
            ));
        }

        let img = &*(ximage as *const XImageStruct);
        
        // 确保缓冲区已分配
        if self.frame_buffer.is_none() {
            self.allocate_buffer();
        }

        let buffer = self.frame_buffer.as_mut().unwrap();
        let src_data = std::slice::from_raw_parts(
            img.data as *const u8,
            (img.height * img.bytes_per_line) as usize,
        );

        // 复制数据
        let dst_stride = (self.width * 4) as usize;
        let src_stride = img.bytes_per_line as usize;
        let copy_width = std::cmp::min(dst_stride, src_stride);
        let copy_height = std::cmp::min(self.height as usize, img.height as usize);

        for y in 0..copy_height {
            let dst_offset = y * dst_stride;
            let src_offset = y * src_stride;
            buffer[dst_offset..dst_offset + copy_width]
                .copy_from_slice(&src_data[src_offset..src_offset + copy_width]);
        }

        // 释放 XImage
        if let Some(destroy_image) = self.funcs.destroy_image {
            destroy_image(ximage);
        }

        Ok(())
    }
}

impl CaptureBackend for X11Capture {
    fn init(&mut self) -> Result<(), CaptureError> {
        if self.initialized {
            return Ok(());
        }

        unsafe {
            // 打开 X 显示连接
            if let Some(open_display) = self.funcs.open_display {
                self.display = open_display(null_mut());
            }

            if self.display.is_null() {
                return Err(CaptureError::X11Error(
                    "无法连接到 X 服务器".to_string(),
                ));
            }

            // 获取默认屏幕
            if let Some(default_screen) = self.funcs.default_screen {
                self.screen = default_screen(self.display);
            }

            // 获取根窗口
            if let Some(default_root) = self.funcs.default_root_window {
                self.root_window = default_root(self.display);
            }

            // 获取屏幕分辨率
            if let Some(display_width) = self.funcs.display_width {
                self.width = display_width(self.display, self.screen) as u32;
            }
            if let Some(display_height) = self.funcs.display_height {
                self.height = display_height(self.display, self.screen) as u32;
            }
        }

        // 分配帧缓冲区
        self.allocate_buffer();

        self.initialized = true;
        log::info!(
            "X11 捕获初始化成功: {}x{}",
            self.width,
            self.height
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
            if let Some(get_image) = self.funcs.get_image {
                let ximage = get_image(
                    self.display,
                    self.root_window,
                    0,
                    0,
                    self.width as c_uint,
                    self.height as c_uint,
                    AllPlanes,
                    ZPixmap,
                );

                self.copy_from_ximage(ximage)?;
            }
        }

        // X11 不是真正的零拷贝，我们返回一个模拟的 FrameRef
        // 实际使用时，gpu_ptr 和 dmabuf_fd 都为 None
        let mut frame = FrameRef::new(self.width, self.height, self.format);
        
        // 如果有缓冲区数据，可以在这里设置一个指向缓冲区的指针
        // 注意：这不是真正的 GPU 指针，只是系统内存指针
        if let Some(ref buffer) = self.frame_buffer {
            frame.gpu_ptr = Some(buffer.as_ptr() as usize);
        }

        Ok(frame)
    }

    fn is_zero_copy(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "X11"
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_resolution(&mut self, width: u32, height: u32) -> Result<(), CaptureError> {
        self.width = width;
        self.height = height;
        
        // 重新分配缓冲区
        self.allocate_buffer();
        
        Ok(())
    }

    fn cleanup(&mut self) {
        unsafe {
            // 释放缓冲区
            self.frame_buffer = None;

            // 关闭 X 显示连接
            if !self.display.is_null() {
                if let Some(close_display) = self.funcs.close_display {
                    close_display(self.display);
                }
                self.display = null_mut();
            }

            // 关闭 X11 库
            if !self.x11_lib.is_null() {
                libc::dlclose(self.x11_lib);
                self.x11_lib = null_mut();
            }
        }

        self.initialized = false;
        log::info!("X11 捕获已清理");
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        if self.initialized {
            self.cleanup();
        }
    }
}

unsafe impl Send for X11Capture {}
unsafe impl Sync for X11Capture {}

#[cfg(test)]
mod x11_tests {
    use super::*;

    #[test]
    fn test_x11_is_available() {
        let _available = X11Capture::is_available();
    }

    #[test]
    fn test_x11_capture_properties() {
        if !X11Capture::is_available() {
            println!("跳过测试：X11 不可用");
            return;
        }

        match X11Capture::new() {
            Ok(capture) => {
                assert_eq!(capture.name(), "X11");
                assert!(!capture.is_zero_copy()); // X11 不是零拷贝
            }
            Err(_) => {
                println!("X11 创建失败（在无显示系统上是正常的）");
            }
        }
    }

    #[test]
    fn test_x11_resolution() {
        if !X11Capture::is_available() {
            println!("跳过测试：X11 不可用");
            return;
        }

        match X11Capture::new() {
            Ok(mut capture) => {
                // 测试设置分辨率
                let result = capture.set_resolution(1280, 720);
                assert!(result.is_ok());
                assert_eq!(capture.resolution(), (1280, 720));
            }
            Err(_) => {
                println!("X11 创建失败");
            }
        }
    }
}
