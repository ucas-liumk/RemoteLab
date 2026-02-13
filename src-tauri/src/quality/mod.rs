//! 自适应质量控制模块 - Quality Agent
//!
//! 职责：
//! 1. 网络质量监测与带宽估计
//! 2. 自适应码率控制 (ABR) - 基于 Steam Link 算法
//! 3. 编码参数动态调整
//!
//! 模块结构：
//! - monitor: 网络质量监测 (RTT, 丢包, 抖动)
//! - adaptive_controller: 自适应码率控制算法
//! - bandwidth_estimator: 带宽估计器 (GCC 算法简化版)
//! - tests: 单元测试

use std::collections::VecDeque;
use std::time::{Duration, Instant};

// 子模块声明
pub mod monitor;
pub mod adaptive_controller;
pub mod bandwidth_estimator;

#[cfg(test)]
mod tests;

// 重新导出常用类型
pub use monitor::NetworkMonitor;
pub use adaptive_controller::{
    AdaptiveBitrateController, ControllerState, QualityController,
};
pub use bandwidth_estimator::BandwidthEstimator;

/// 编解码器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    H264,
    HEVC,
    AV1,
}

impl CodecType {
    /// 获取编解码器名称
    pub fn as_str(&self) -> &'static str {
        match self {
            CodecType::H264 => "H264",
            CodecType::HEVC => "HEVC",
            CodecType::AV1 => "AV1",
        }
    }
}

/// 质量设置
#[derive(Debug, Clone)]
pub struct QualitySettings {
    /// 目标码率 (bps)
    pub bitrate: u32,
    /// 目标帧率
    pub fps: u32,
    /// 量化参数 (0-51，越小质量越好)
    pub qp: u32,
    /// 分辨率
    pub resolution: (u32, u32),
    /// 编解码器类型
    pub codec: CodecType,
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self {
            bitrate: 20_000_000, // 20 Mbps
            fps: 60,
            qp: 28,
            resolution: (1920, 1080),
            codec: CodecType::H264,
        }
    }
}

/// 网络统计
#[derive(Debug, Clone, Copy)]
pub struct NetworkStats {
    /// RTT (毫秒)
    pub rtt_ms: f32,
    /// 抖动 (毫秒)
    pub jitter_ms: f32,
    /// 丢包率 (0-1)
    pub loss_rate: f32,
    /// 带宽估计 (bps)
    pub bandwidth_bps: u32,
    /// 时间戳
    pub timestamp: Instant,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            rtt_ms: 0.0,
            jitter_ms: 0.0,
            loss_rate: 0.0,
            bandwidth_bps: 10_000_000,
            timestamp: Instant::now(),
        }
    }
}

/// 编码器回调类型
pub type EncoderCallback = Box<dyn Fn(QualitySettings) + Send>;

/// 质量控制器 trait
///
/// 基于网络状况动态调整编码参数
pub trait QualityControllerTrait: Send + Sync {
    /// 更新网络统计
    fn update_stats(&mut self, stats: NetworkStats);

    /// 获取当前质量设置
    fn get_quality(&self) -> QualitySettings;

    /// 注册编码器控制回调
    fn register_encoder_callback(&mut self, callback: EncoderCallback);
}

/// 计算百分位值
pub fn percentile(sorted_data: &[f32], p: f32) -> f32 {
    if sorted_data.is_empty() {
        return 0.0;
    }
    if sorted_data.len() == 1 {
        return sorted_data[0];
    }

    let index = (sorted_data.len() as f32 * p) as usize;
    let clamped_index = index.clamp(0, sorted_data.len() - 1);
    sorted_data[clamped_index]
}

/// 网络质量评分 (0-100)
pub fn calculate_quality_score(rtt_ms: f32, loss_rate: f32, jitter_ms: f32) -> u8 {
    // 基于 RTT、丢包率和抖动计算综合评分
    let rtt_score = if rtt_ms < 20.0 {
        40
    } else if rtt_ms < 50.0 {
        30
    } else if rtt_ms < 100.0 {
        20
    } else {
        10
    };

    let loss_score = if loss_rate < 0.001 {
        30
    } else if loss_rate < 0.01 {
        20
    } else if loss_rate < 0.05 {
        10
    } else {
        0
    };

    let jitter_score = if jitter_ms < 5.0 {
        30
    } else if jitter_ms < 15.0 {
        20
    } else if jitter_ms < 30.0 {
        10
    } else {
        0
    };

    (rtt_score + loss_score + jitter_score).min(100) as u8
}

/// 创建默认质量控制器
pub fn create_quality_controller(initial_bitrate: u32) -> AdaptiveBitrateController {
    AdaptiveBitrateController::new(initial_bitrate)
}
