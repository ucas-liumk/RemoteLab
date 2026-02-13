//! 视频编码模块 - Encoder Agent
//!
//! 职责：
//! 1. 将 FrameRef 编码为压缩视频流
//! 2. 支持 GPU 硬件编码 (NVENC/VA-API)
//! 3. 自适应码率控制接口

use crate::capture::{FrameRef, PixelFormat};
use bytes::Bytes;
use thiserror::Error;

/// 视频编码格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// H.264 - 最广泛支持
    H264,
    /// HEVC/H.265 - 更好压缩比
    HEVC,
    /// AV1 - 下一代编解码器
    AV1,
}

impl VideoCodec {
    /// MIME 类型
    pub fn mime_type(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "video/h264",
            VideoCodec::HEVC => "video/hevc",
            VideoCodec::AV1 => "video/av1",
        }
    }
}

/// 帧类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// 关键帧 - 可独立解码 (IDR)
    IDR,
    /// 普通 I 帧
    I,
    /// 前向预测帧
    P,
    /// 双向预测帧 (尽量避免，增加延迟)
    B,
}

impl FrameType {
    /// 是否为关键帧
    pub fn is_keyframe(&self) -> bool {
        matches!(self, FrameType::IDR | FrameType::I)
    }
}

/// 编码质量预设
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    /// 追求最低编码延迟
    UltraFast,
    /// 快速编码
    Fast,
    /// 平衡质量与速度
    Balanced,
    /// 最佳画质 (允许更高延迟)
    Quality,
}

/// 编码后的视频帧
///
/// 包含压缩后的视频数据和元信息
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    /// 帧 ID (与 FrameRef.frame_id 对应)
    pub frame_id: u64,
    
    /// 编码时间戳 (微秒)
    pub encode_timestamp_us: u64,
    
    /// 编码耗时 (微秒) - 用于质量评估
    pub encode_duration_us: u64,
    
    /// 编码格式
    pub codec: VideoCodec,
    
    /// 帧类型
    pub frame_type: FrameType,
    
    /// 压缩后的视频数据 (通常 1-50KB)
    ///
    /// 使用 `Bytes` 实现零拷贝克隆
    pub data: Bytes,
    
    /// 原始帧尺寸
    pub width: u32,
    pub height: u32,
    
    /// 当前码率 (bps)
    pub bitrate: u32,
    
    /// 量化参数 (质量指标，越小越好，范围 0-51)
    pub qp: u8,
    
    /// 是否为场景切换帧
    pub scene_change: bool,
    
    /// 渲染时间戳 (用于同步)
    pub pts: u64,
}

/// 编码器配置
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// 编码格式
    pub codec: VideoCodec,
    /// 视频宽度
    pub width: u32,
    /// 视频高度
    pub height: u32,
    /// 目标帧率
    pub framerate: u32,
    /// 初始码率 (bps)
    pub bitrate_bps: u32,
    /// 关键帧间隔 (GOP 长度)
    pub keyframe_interval: u32,
    /// 低延迟模式 (禁用 B 帧，降低缓冲)
    pub low_latency_mode: bool,
    /// 质量预设
    pub quality_preset: QualityPreset,
    /// 输入像素格式
    pub input_format: PixelFormat,
    /// 最大码率 (用于 VBR)
    pub max_bitrate_bps: Option<u32>,
    /// 最小量化参数
    pub min_qp: Option<u8>,
    /// 最大量化参数
    pub max_qp: Option<u8>,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate_bps: 10_000_000, // 10 Mbps
            keyframe_interval: 60,   // 1 second @ 60fps
            low_latency_mode: true,
            quality_preset: QualityPreset::UltraFast,
            input_format: PixelFormat::Bgra8Unorm,
            max_bitrate_bps: None,
            min_qp: None,
            max_qp: None,
        }
    }
}

/// 编码器统计
#[derive(Debug, Default, Clone)]
pub struct EncoderStats {
    /// 总编码帧数
    pub frames_encoded: u64,
    /// 关键帧数量
    pub keyframes_encoded: u64,
    /// 平均编码时间 (微秒)
    pub average_encode_time_us: u64,
    /// 当前码率
    pub current_bitrate: u32,
    /// 当前帧率
    pub current_framerate: f32,
    /// 平均帧大小 (字节)
    pub average_frame_size: usize,
    /// 编码错误数
    pub encode_errors: u64,
}

/// 编码器错误
#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Encoder initialization failed: {0}")]
    InitFailed(String),
    
    #[error("Hardware encoder unavailable")]
    HardwareUnavailable,
    
    #[error("Encode failed: {0}")]
    EncodeFailed(String),
    
    #[error("Invalid input format: {0:?}")]
    InvalidInputFormat(PixelFormat),
    
    #[error("Resolution not supported: {0}x{1}")]
    ResolutionNotSupported(u32, u32),
    
    #[error("Out of memory")]
    OutOfMemory,
}

/// 编码器 trait
#[async_trait::async_trait]
pub trait VideoEncoder: Send + Sync {
    /// 初始化编码器
    async fn initialize(&mut self, config: EncoderConfig) -> Result<(), EncoderError>;
    
    /// 编码帧
    ///
    /// 接受 FrameRef (GPU 内存引用)，返回 EncodedFrame
    /// 实现零拷贝: GPU → NVENC → 压缩数据
    async fn encode(&mut self, frame: &FrameRef) -> Result<EncodedFrame, EncoderError>;
    
    /// 刷新编码器 (获取剩余帧)
    async fn flush(&mut self) -> Result<Vec<EncodedFrame>, EncoderError>;
    
    /// 请求 IDR 帧 (用于错误恢复)
    fn request_idr(&mut self);
    
    /// 动态更新码率
    fn set_bitrate(&mut self, bitrate_bps: u32);
    
    /// 动态更新帧率
    fn set_framerate(&mut self, framerate: u32);
    
    /// 获取编码器统计
    fn get_stats(&self) -> EncoderStats;
    
    /// 关闭编码器
    async fn shutdown(&mut self) -> Result<(), EncoderError>;
    
    /// 获取编码器名称
    fn name(&self) -> &'static str;
    
    /// 是否硬件加速
    fn is_hardware(&self) -> bool;
}

/// 编码建议 (来自 Quality Controller)
#[derive(Debug, Clone)]
pub struct EncodingHints {
    /// 目标码率 (bps)
    pub target_bitrate: u32,
    /// 目标帧率
    pub target_framerate: u32,
    /// 最大量化参数
    pub max_qp: u8,
    /// 是否启用 FEC
    pub enable_fec: bool,
    /// FEC 冗余比例
    pub fec_ratio: f32,
    /// 是否降分辨率
    pub scale_resolution: Option<(u32, u32)>,
    /// 建议的编码策略
    pub strategy: EncodingStrategy,
    /// 强制 IDR 帧
    pub force_idr: bool,
}

impl Default for EncodingHints {
    fn default() -> Self {
        Self {
            target_bitrate: 10_000_000,
            target_framerate: 60,
            max_qp: 51,
            enable_fec: false,
            fec_ratio: 0.0,
            scale_resolution: None,
            strategy: EncodingStrategy::MinLatency,
            force_idr: false,
        }
    }
}

/// 编码策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingStrategy {
    /// 追求最低延迟 (牺牲部分质量)
    MinLatency,
    /// 平衡模式
    Balanced,
    /// 追求最高质量 (允许稍高延迟)
    MaxQuality,
}

/// 创建最佳可用编码器
pub fn create_encoder() -> Box<dyn VideoEncoder> {
    // TODO: 检测 GPU 并选择最佳编码器
    // 1. 尝试 NVENC (NVIDIA)
    // 2. 尝试 VA-API (AMD/Intel on Linux)
    // 3. 尝试 Media Foundation (Windows)
    // 4. 尝试 VideoToolbox (macOS)
    // 5. 回退到软件编码 (OpenH264)
    
    Box::new(software::SoftwareEncoder::new())
}

// 子模块
pub mod software;

#[cfg(feature = "nvenc")]
pub mod nvenc;

#[cfg(all(target_os = "linux", feature = "vaapi"))]
pub mod vaapi;
