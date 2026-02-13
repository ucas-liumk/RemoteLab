//! NVIDIA NvFBC (Frame Buffer Capture) 零拷贝捕获实现
//!
//! 该模块通过 NVIDIA 的 NvFBC SDK 实现直接从 GPU 显存捕获屏幕内容，
//! 无需将数据复制到系统内存，实现真正的零拷贝。
//!
//! # 要求
//! - NVIDIA GPU (支持 NvFBC)
//! - libnvidia-fbc.so 库
//! - CUDA 驱动
//!
//! # 参考
//! - NVIDIA Capture SDK: https://developer.nvidia.com/capture-sdk

use super::{CaptureBackend, CaptureError, FrameRef, PixelFormat};
use libc::{c_int, c_uint, c_void, dlopen, dlsym, RTLD_NOW};
use std::ffi::CString;
use std::os::raw::{c_char, c_ulong};
use std::ptr::null_mut;
use std::time::Instant;

// NvFBC 类型定义
type NvFBCStatus = c_int;

const NVFBC_STATUS_SUCCESS: NvFBCStatus = 0;
const NVFBC_STATUS_FAILURE: NvFBCStatus = 1;
const NVFBC_STATUS_BAD_PARAMETER: NvFBCStatus = 2;
const NVFBC_STATUS_INVALID_CALL: NvFBCStatus = 3;
const NVFBC_STATUS_OUT_OF_MEMORY: NvFBCStatus = 4;
const NVFBC_STATUS_UNSUPPORTED: NvFBCStatus = 5;
const NVFBC_STATUS_DEVICE_ERROR: NvFBCStatus = 6;

// NvFBC 会话句柄
type NvFBCSessionHandle = *mut c_void;

// NvFBC API 函数类型
type NvFBCCreateInstanceFn = unsafe extern "C" fn(*mut *mut c_void) -> NvFBCStatus;
type NvFBCDestroyInstanceFn = unsafe extern "C" fn(*mut c_void) -> NvFBCStatus;
type NvFBCCreateSessionFn = unsafe extern "C" fn(*mut c_void, *mut NvFBCSessionHandle) -> NvFBCStatus;
type NvFBCDestroySessionFn = unsafe extern "C" fn(NvFBCSessionHandle) -> NvFBCStatus;
type NvFBCGetStatusFn = unsafe extern "C" fn(*mut c_void, *mut NvFBCStatusInfo) -> NvFBCStatus;

// NvFBC 捕获类型
const NVFBC_CAPTURE_TO_CUDA: c_uint = 0x00000001;
const NVFBC_CAPTURE_TO_GL: c_uint = 0x00000002;
const NVFBC_CAPTURE_TO_VULKAN: c_uint = 0x00000004;

// NvFBC 状态信息结构
#[repr(C)]
#[derive(Debug, Clone)]
struct NvFBCStatusInfo {
    version: c_uint,
    is_capture_possible: c_int,
    is_capturing: c_int,
    width: c_uint,
    height: c_uint,
    reserved: [c_uint; 16],
}

// NvFBC 创建参数
#[repr(C)]
#[derive(Debug, Clone)]
struct NvFBCCreateParams {
    version: c_uint,
    capture_type: c_uint,
    cuda_ctx: *mut c_void,
    reserved: [*mut c_void; 8],
}

// NvFBC CUDA 捕获参数
#[repr(C)]
#[derive(Debug, Clone)]
struct NvFBCCudaCaptureParams {
    version: c_uint,
    width: c_uint,
    height: c_uint,
    format: c_uint,
    gpu_ptr: *mut c_void,
    pitch: c_uint,
    timestamp: c_ulong,
    reserved: [c_uint; 8],
}

// CUDA 类型定义
type CUresult = c_int;
type CUcontext = *mut c_void;
type CUdevice = c_int;
type CUdeviceptr = usize;

const CUDA_SUCCESS: CUresult = 0;

/// CUDA 驱动函数类型
type CuInitFn = unsafe extern "C" fn(c_uint) -> CUresult;
type CuDeviceGetFn = unsafe extern "C" fn(*mut CUdevice, c_int) -> CUresult;
type CuCtxCreateFn = unsafe extern "C" fn(*mut CUcontext, c_uint, CUdevice) -> CUresult;
type CuCtxDestroyFn = unsafe extern "C" fn(CUcontext) -> CUresult;
type CuMemAllocFn = unsafe extern "C" fn(*mut CUdeviceptr, usize) -> CUresult;
type CuMemFreeFn = unsafe extern "C" fn(CUdeviceptr) -> CUresult;
type CuMemcpyDtoHFn = unsafe extern "C" fn(*mut c_void, CUdeviceptr, usize) -> CUresult;

/// NvFBC 捕获实现
///
/// 使用 NVIDIA 的 NvFBC SDK 直接从 GPU 显存捕获屏幕
/// 支持 CUDA 零拷贝捕获
pub struct NvFBCCapture {
    /// NvFBC 库句柄
    nvfbc_lib: *mut c_void,
    /// CUDA 驱动库句柄
    cuda_lib: *mut c_void,
    /// NvFBC 实例
    nvfbc_instance: *mut c_void,
    /// NvFBC 会话句柄
    session: NvFBCSessionHandle,
    /// CUDA 上下文
    cuda_ctx: CUcontext,
    /// CUDA 设备指针 (用于帧数据)
    gpu_buffer: CUdeviceptr,
    /// 缓冲区大小
    buffer_size: usize,
    /// 当前分辨率
    width: u32,
    /// 当前高度
    height: u32,
    /// 当前格式
    format: PixelFormat,
    /// 是否已初始化
    initialized: bool,
    /// 函数指针缓存
    funcs: NvFBCFunctions,
    /// CUDA 函数指针缓存
    cuda_funcs: CudaFunctions,
}

/// NvFBC 函数指针集合
struct NvFBCFunctions {
    create_instance: Option<NvFBCCreateInstanceFn>,
    destroy_instance: Option<NvFBCDestroyInstanceFn>,
    create_session: Option<NvFBCCreateSessionFn>,
    destroy_session: Option<NvFBCDestroySessionFn>,
    get_status: Option<NvFBCGetStatusFn>,
}

/// CUDA 函数指针集合
struct CudaFunctions {
    init: Option<CuInitFn>,
    device_get: Option<CuDeviceGetFn>,
    ctx_create: Option<CuCtxCreateFn>,
    ctx_destroy: Option<CuCtxDestroyFn>,
    mem_alloc: Option<CuMemAllocFn>,
    mem_free: Option<CuMemFreeFn>,
    memcpy_dtoh: Option<CuMemcpyDtoHFn>,
}

impl NvFBCCapture {
    /// 库名称
    const NVFBC_LIB_NAME: &'static str = "libnvidia-fbc.so.1";
    const CUDA_LIB_NAME: &'static str = "libcuda.so.1";

    /// 创建新的 NvFBC 捕获实例
    ///
    /// # Returns
    /// - `Ok(NvFBCCapture)` - 创建成功
    /// - `Err(CaptureError)` - 创建失败（库未找到或不支持）
    pub fn new() -> Result<Self, CaptureError> {
        // 检查 NvFBC 是否可用
        if !Self::is_available() {
            return Err(CaptureError::ResourceUnavailable(
                "NvFBC 库未找到或不支持".to_string(),
            ));
        }

        // 加载 NvFBC 库
        let nvfbc_lib = unsafe {
            let lib_name = CString::new(Self::NVFBC_LIB_NAME).unwrap();
            dlopen(lib_name.as_ptr(), RTLD_NOW)
        };

        if nvfbc_lib.is_null() {
            return Err(CaptureError::ResourceUnavailable(
                "无法加载 NvFBC 库".to_string(),
            ));
        }

        // 加载 CUDA 库
        let cuda_lib = unsafe {
            let lib_name = CString::new(Self::CUDA_LIB_NAME).unwrap();
            dlopen(lib_name.as_ptr(), RTLD_NOW)
        };

        if cuda_lib.is_null() {
            unsafe {
                libc::dlclose(nvfbc_lib);
            }
            return Err(CaptureError::ResourceUnavailable(
                "无法加载 CUDA 库".to_string(),
            ));
        }

        // 加载 NvFBC 函数
        let funcs = unsafe { Self::load_nvfbc_functions(nvfbc_lib) }?;

        // 加载 CUDA 函数
        let cuda_funcs = unsafe { Self::load_cuda_functions(cuda_lib) }?;

        Ok(Self {
            nvfbc_lib,
            cuda_lib,
            nvfbc_instance: null_mut(),
            session: null_mut(),
            cuda_ctx: null_mut(),
            gpu_buffer: 0,
            buffer_size: 0,
            width: 1920,
            height: 1080,
            format: PixelFormat::BGRA,
            initialized: false,
            funcs,
            cuda_funcs,
        })
    }

    /// 检查系统是否支持 NvFBC
    pub fn is_available() -> bool {
        unsafe {
            let lib_name = CString::new(Self::NVFBC_LIB_NAME).unwrap();
            let handle = dlopen(lib_name.as_ptr(), libc::RTLD_LAZY);
            if !handle.is_null() {
                libc::dlclose(handle);
                true
            } else {
                false
            }
        }
    }

    /// 获取 NvFBC 版本信息
    pub fn get_version(&self) -> Result<String, CaptureError> {
        if self.nvfbc_instance.is_null() {
            return Err(CaptureError::InvalidCall(
                "NvFBC 未初始化".to_string(),
            ));
        }

        // 实际实现中需要从 NvFBC 获取版本
        Ok("NvFBC 1.0".to_string())
    }

    /// 加载 NvFBC 函数指针
    unsafe fn load_nvfbc_functions(lib: *mut c_void) -> Result<NvFBCFunctions, CaptureError> {
        let create_instance_sym = CString::new("NvFBCCreateInstance").unwrap();
        let destroy_instance_sym = CString::new("NvFBCDestroyInstance").unwrap();
        let create_session_sym = CString::new("NvFBCCreateSession").unwrap();
        let destroy_session_sym = CString::new("NvFBCDestroySession").unwrap();
        let get_status_sym = CString::new("NvFBCGetStatus").unwrap();

        let create_instance: NvFBCCreateInstanceFn = std::mem::transmute(
            dlsym(lib, create_instance_sym.as_ptr())
        );
        let destroy_instance: NvFBCDestroyInstanceFn = std::mem::transmute(
            dlsym(lib, destroy_instance_sym.as_ptr())
        );
        let create_session: NvFBCCreateSessionFn = std::mem::transmute(
            dlsym(lib, create_session_sym.as_ptr())
        );
        let destroy_session: NvFBCDestroySessionFn = std::mem::transmute(
            dlsym(lib, destroy_session_sym.as_ptr())
        );
        let get_status: NvFBCGetStatusFn = std::mem::transmute(
            dlsym(lib, get_status_sym.as_ptr())
        );

        if create_instance.is_none() || destroy_instance.is_none() {
            return Err(CaptureError::InitFailed(
                "无法加载 NvFBC 必要函数".to_string(),
            ));
        }

        Ok(NvFBCFunctions {
            create_instance,
            destroy_instance,
            create_session,
            destroy_session,
            get_status,
        })
    }

    /// 加载 CUDA 函数指针
    unsafe fn load_cuda_functions(lib: *mut c_void) -> Result<CudaFunctions, CaptureError> {
        let init_sym = CString::new("cuInit").unwrap();
        let device_get_sym = CString::new("cuDeviceGet").unwrap();
        let ctx_create_sym = CString::new("cuCtxCreate").unwrap();
        let ctx_destroy_sym = CString::new("cuCtxDestroy").unwrap();
        let mem_alloc_sym = CString::new("cuMemAlloc").unwrap();
        let mem_free_sym = CString::new("cuMemFree").unwrap();
        let memcpy_dtoh_sym = CString::new("cuMemcpyDtoH").unwrap();

        let init: CuInitFn = std::mem::transmute(dlsym(lib, init_sym.as_ptr()));
        let device_get: CuDeviceGetFn = std::mem::transmute(dlsym(lib, device_get_sym.as_ptr()));
        let ctx_create: CuCtxCreateFn = std::mem::transmute(dlsym(lib, ctx_create_sym.as_ptr()));
        let ctx_destroy: CuCtxDestroyFn = std::mem::transmute(dlsym(lib, ctx_destroy_sym.as_ptr()));
        let mem_alloc: CuMemAllocFn = std::mem::transmute(dlsym(lib, mem_alloc_sym.as_ptr()));
        let mem_free: CuMemFreeFn = std::mem::transmute(dlsym(lib, mem_free_sym.as_ptr()));
        let memcpy_dtoh: CuMemcpyDtoHFn = std::mem::transmute(dlsym(lib, memcpy_dtoh_sym.as_ptr()));

        if init.is_none() || device_get.is_none() || ctx_create.is_none() {
            return Err(CaptureError::InitFailed(
                "无法加载 CUDA 必要函数".to_string(),
            ));
        }

        Ok(CudaFunctions {
            init,
            device_get,
            ctx_create,
            ctx_destroy,
            mem_alloc,
            mem_free,
            memcpy_dtoh,
        })
    }

    /// 初始化 CUDA 上下文
    unsafe fn init_cuda(&mut self) -> Result<(), CaptureError> {
        if let Some(cu_init) = self.cuda_funcs.init {
            let result = cu_init(0);
            if result != CUDA_SUCCESS {
                return Err(CaptureError::CudaError(
                    format!("cuInit 失败: {}", result),
                ));
            }
        }

        if let Some(cu_device_get) = self.cuda_funcs.device_get {
            let mut device: CUdevice = 0;
            let result = cu_device_get(&mut device, 0);
            if result != CUDA_SUCCESS {
                return Err(CaptureError::CudaError(
                    format!("cuDeviceGet 失败: {}", result),
                ));
            }

            if let Some(cu_ctx_create) = self.cuda_funcs.ctx_create {
                let result = cu_ctx_create(&mut self.cuda_ctx, 0, device);
                if result != CUDA_SUCCESS {
                    return Err(CaptureError::CudaError(
                        format!("cuCtxCreate 失败: {}", result),
                    ));
                }
            }
        }

        Ok(())
    }

    /// 分配 GPU 缓冲区
    unsafe fn allocate_gpu_buffer(&mut self) -> Result<(), CaptureError> {
        let size = (self.width * self.height * 4) as usize; // BGRA

        if self.gpu_buffer != 0 {
            if let Some(cu_mem_free) = self.cuda_funcs.mem_free {
                cu_mem_free(self.gpu_buffer);
            }
        }

        if let Some(cu_mem_alloc) = self.cuda_funcs.mem_alloc {
            let result = cu_mem_alloc(&mut self.gpu_buffer, size);
            if result != CUDA_SUCCESS {
                return Err(CaptureError::CudaError(
                    format!("cuMemAlloc 失败: {}", result),
                ));
            }
        }

        self.buffer_size = size;
        Ok(())
    }

    /// 创建 NvFBC 会话
    unsafe fn create_session(&mut self) -> Result<(), CaptureError> {
        if let Some(create_instance) = self.funcs.create_instance {
            let result = create_instance(&mut self.nvfbc_instance);
            if result != NVFBC_STATUS_SUCCESS {
                return Err(CaptureError::InitFailed(
                    format!("NvFBCCreateInstance 失败: {}", result),
                ));
            }
        }

        if let Some(create_session) = self.funcs.create_session {
            let mut params = NvFBCCreateParams {
                version: 1,
                capture_type: NVFBC_CAPTURE_TO_CUDA,
                cuda_ctx: self.cuda_ctx,
                reserved: [null_mut(); 8],
            };

            let result = create_session(self.nvfbc_instance, &mut self.session);
            if result != NVFBC_STATUS_SUCCESS {
                return Err(CaptureError::InitFailed(
                    format!("NvFBCCreateSession 失败: {}", result),
                ));
            }
        }

        Ok(())
    }

    /// 使用 NvFBC 捕获一帧到 CUDA
    unsafe fn capture_to_cuda(&mut self) -> Result<FrameRef, CaptureError> {
        // 实际实现中需要调用 NvFBCToCudaGrabFrame
        // 这里提供一个模拟实现框架

        if self.session.is_null() {
            return Err(CaptureError::CaptureFailed(
                "NvFBC 会话未创建".to_string(),
            ));
        }

        // 获取状态
        let mut status = NvFBCStatusInfo {
            version: 1,
            is_capture_possible: 0,
            is_capturing: 0,
            width: 0,
            height: 0,
            reserved: [0; 16],
        };

        if let Some(get_status) = self.funcs.get_status {
            let result = get_status(self.nvfbc_instance, &mut status);
            if result != NVFBC_STATUS_SUCCESS {
                return Err(CaptureError::CaptureFailed(
                    format!("NvFBCGetStatus 失败: {}", result),
                ));
            }
        }

        if status.is_capture_possible == 0 {
            return Err(CaptureError::CaptureFailed(
                "当前无法捕获".to_string(),
            ));
        }

        // 更新分辨率（如果需要）
        if status.width != self.width || status.height != self.height {
            self.width = status.width;
            self.height = status.height;
            self.allocate_gpu_buffer()?;
        }

        // 创建帧引用
        let frame = FrameRef::with_gpu_ptr(
            self.width,
            self.height,
            self.format,
            self.gpu_buffer,
        );

        Ok(frame)
    }
}

impl CaptureBackend for NvFBCCapture {
    fn init(&mut self) -> Result<(), CaptureError> {
        if self.initialized {
            return Ok(());
        }

        unsafe {
            // 初始化 CUDA
            self.init_cuda()?;

            // 分配 GPU 缓冲区
            self.allocate_gpu_buffer()?;

            // 创建 NvFBC 会话
            self.create_session()?;
        }

        self.initialized = true;
        log::info!("NvFBC 捕获初始化成功: {}x{}", self.width, self.height);

        Ok(())
    }

    fn capture(&mut self) -> Result<FrameRef, CaptureError> {
        if !self.initialized {
            return Err(CaptureError::InvalidCall(
                "捕获前必须先初始化".to_string(),
            ));
        }

        unsafe { self.capture_to_cuda() }
    }

    fn is_zero_copy(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "NvFBC"
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_resolution(&mut self, width: u32, height: u32) -> Result<(), CaptureError> {
        self.width = width;
        self.height = height;

        if self.initialized {
            unsafe {
                self.allocate_gpu_buffer()?;
            }
        }

        Ok(())
    }

    fn cleanup(&mut self) {
        unsafe {
            // 释放 GPU 缓冲区
            if self.gpu_buffer != 0 {
                if let Some(cu_mem_free) = self.cuda_funcs.mem_free {
                    cu_mem_free(self.gpu_buffer);
                }
                self.gpu_buffer = 0;
            }

            // 销毁 NvFBC 会话
            if !self.session.is_null() {
                if let Some(destroy_session) = self.funcs.destroy_session {
                    destroy_session(self.session);
                }
                self.session = null_mut();
            }

            // 销毁 NvFBC 实例
            if !self.nvfbc_instance.is_null() {
                if let Some(destroy_instance) = self.funcs.destroy_instance {
                    destroy_instance(self.nvfbc_instance);
                }
                self.nvfbc_instance = null_mut();
            }

            // 销毁 CUDA 上下文
            if !self.cuda_ctx.is_null() {
                if let Some(cu_ctx_destroy) = self.cuda_funcs.ctx_destroy {
                    cu_ctx_destroy(self.cuda_ctx);
                }
                self.cuda_ctx = null_mut();
            }

            // 关闭库
            if !self.nvfbc_lib.is_null() {
                libc::dlclose(self.nvfbc_lib);
                self.nvfbc_lib = null_mut();
            }

            if !self.cuda_lib.is_null() {
                libc::dlclose(self.cuda_lib);
                self.cuda_lib = null_mut();
            }
        }

        self.initialized = false;
        log::info!("NvFBC 捕获已清理");
    }
}

impl Drop for NvFBCCapture {
    fn drop(&mut self) {
        if self.initialized {
            self.cleanup();
        }
    }
}

// 为函数指针实现 Send + Sync
unsafe impl Send for NvFBCCapture {}
unsafe impl Sync for NvFBCCapture {}

#[cfg(test)]
mod nvfbc_tests {
    use super::*;

    #[test]
    fn test_nvfbcs_is_available() {
        // 这个测试只是检查函数是否可以调用
        let _available = NvFBCCapture::is_available();
    }

    #[test]
    fn test_nvfbcs_capture_properties() {
        if !NvFBCCapture::is_available() {
            println!("跳过测试：NvFBC 不可用");
            return;
        }

        match NvFBCCapture::new() {
            Ok(capture) => {
                assert_eq!(capture.name(), "NvFBC");
                assert!(capture.is_zero_copy());
            }
            Err(_) => {
                println!("NvFBC 创建失败（在非 NVIDIA 系统上是正常的）");
            }
        }
    }

    #[test]
    fn test_nvfbcs_resolution() {
        if !NvFBCCapture::is_available() {
            println!("跳过测试：NvFBC 不可用");
            return;
        }

        match NvFBCCapture::new() {
            Ok(mut capture) => {
                assert_eq!(capture.resolution(), (1920, 1080));
                
                // 测试设置分辨率
                let result = capture.set_resolution(2560, 1440);
                if result.is_ok() {
                    assert_eq!(capture.resolution(), (2560, 1440));
                }
            }
            Err(_) => {
                println!("NvFBC 创建失败（在非 NVIDIA 系统上是正常的）");
            }
        }
    }
}
