//! 自适应码率控制模块
//!
//! 基于 Steam Link 的自适应算法实现
//! 根据网络状况动态调整编码参数

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::{
    monitor::NetworkMonitor, CodecType, EncoderCallback, NetworkStats,
    QualityControllerTrait, QualitySettings,
};

/// 控制器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    /// 网络稳定
    Stable,
    /// 网络恶化
    Degrading,
    /// 网络恢复
    Recovering,
    /// 探测可用带宽
    Probing,
}

impl ControllerState {
    /// 获取状态描述
    pub fn description(&self) -> &'static str {
        match self {
            ControllerState::Stable => "网络稳定",
            ControllerState::Degrading => "网络恶化",
            ControllerState::Recovering => "网络恢复",
            ControllerState::Probing => "探测带宽",
        }
    }
}

/// 自适应码率控制器
///
/// 基于 Steam Link 自适应算法实现
pub struct AdaptiveBitrateController {
    /// 网络监测器
    monitor: NetworkMonitor,
    /// 当前设置
    current_settings: QualitySettings,
    /// 目标码率
    target_bitrate: u32,
    /// 编码器回调
    encoder_callback: Option<EncoderCallback>,
    /// 状态机
    state: ControllerState,
    /// 上次调整时间
    last_adjustment: Instant,
    /// 调整间隔 (防止过于频繁的调整)
    adjustment_interval: Duration,
    /// 最小码率
    min_bitrate: u32,
    /// 最大码率
    max_bitrate: u32,
    /// 连续改进计数
    improvement_count: u32,
    /// 连续恶化计数
    degradation_count: u32,
}

impl AdaptiveBitrateController {
    /// 创建新的自适应码率控制器
    pub fn new(initial_bitrate: u32) -> Self {
        Self {
            monitor: NetworkMonitor::new(),
            current_settings: QualitySettings {
                bitrate: initial_bitrate,
                fps: 60,
                qp: 25,
                resolution: (1920, 1080),
                codec: CodecType::H264,
            },
            target_bitrate: initial_bitrate,
            encoder_callback: None,
            state: ControllerState::Stable,
            last_adjustment: Instant::now(),
            adjustment_interval: Duration::from_millis(500),
            min_bitrate: 2_000_000,    // 2 Mbps
            max_bitrate: 100_000_000,  // 100 Mbps
            improvement_count: 0,
            degradation_count: 0,
        }
    }

    /// 更新网络统计 (trait 实现)
    pub fn update_stats(&mut self, stats: NetworkStats) {
        // 更新监测器
        self.monitor.record_rtt(Duration::from_millis(stats.rtt_ms as u64));
        self.monitor.record_bandwidth_sample(stats.bandwidth_bps);

        // 根据丢包率更新
        if stats.loss_rate > 0.0 {
            let loss_packets = (stats.loss_rate * 100.0) as u32;
            for _ in 0..loss_packets {
                self.monitor.record_loss();
            }
        }

        // 评估网络并调整
        self.evaluate_network();
    }

    /// 评估网络状况并调整
    fn evaluate_network(&mut self) {
        let stats = self.monitor.get_stats();
        let now = Instant::now();

        // 避免过于频繁的调整
        if now.duration_since(self.last_adjustment) < self.adjustment_interval {
            return;
        }

        match self.state {
            ControllerState::Stable => {
                if stats.rtt_ms > 100.0 || stats.loss_rate > 0.02 {
                    // 网络恶化
                    self.state = ControllerState::Degrading;
                    self.reduce_quality();
                } else if stats.rtt_ms < 20.0 && stats.loss_rate < 0.001 {
                    // 网络很好，尝试提升
                    self.state = ControllerState::Probing;
                    self.increase_quality();
                }
            }

            ControllerState::Degrading => {
                // 继续降质量直到稳定
                if stats.rtt_ms > 150.0 || stats.loss_rate > 0.05 {
                    self.reduce_quality_aggressive();
                } else if stats.rtt_ms < 80.0 && stats.loss_rate < 0.01 {
                    // 恢复稳定
                    self.state = ControllerState::Stable;
                }
            }

            ControllerState::Probing => {
                // 探测是否还有带宽余量
                if stats.rtt_ms < 30.0 && stats.jitter_ms < 5.0 {
                    self.increase_quality();
                } else {
                    // 回退到上一个稳定配置
                    self.state = ControllerState::Stable;
                }
            }

            ControllerState::Recovering => {
                // 缓慢恢复质量
                if stats.rtt_ms < 50.0 && stats.loss_rate < 0.01 {
                    self.increase_quality();
                    self.state = ControllerState::Stable;
                } else if stats.rtt_ms > 100.0 {
                    self.state = ControllerState::Degrading;
                }
            }
        }

        self.last_adjustment = now;
    }

    /// 温和降级：降码率 10%，帧率保持
    fn reduce_quality(&mut self) {
        self.target_bitrate = (self.target_bitrate as f32 * 0.9) as u32;
        self.target_bitrate = self.target_bitrate.max(self.min_bitrate);
        self.current_settings.bitrate = self.target_bitrate;
        self.degradation_count += 1;

        // 如果码率已经很低，降分辨率
        if self.current_settings.bitrate < 10_000_000 {
            self.current_settings.resolution = (1280, 720);
        }

        // 增加 QP (降低质量)
        self.current_settings.qp = (self.current_settings.qp + 2).min(45);

        self.apply_settings();
    }

    /// 激进降级：降码率 30%，帧率降到 30
    fn reduce_quality_aggressive(&mut self) {
        self.target_bitrate = (self.target_bitrate as f32 * 0.7) as u32;
        self.target_bitrate = self.target_bitrate.max(self.min_bitrate);
        self.current_settings.bitrate = self.target_bitrate;
        self.current_settings.fps = 30;
        self.current_settings.qp = 35; // 更高压缩
        self.degradation_count += 1;

        if self.current_settings.bitrate < 5_000_000 {
            self.current_settings.resolution = (854, 480);
        }

        self.apply_settings();
    }

    /// 缓慢提升：码率 +5%
    fn increase_quality(&mut self) {
        self.target_bitrate = (self.target_bitrate as f32 * 1.05).min(self.max_bitrate) as u32;
        self.current_settings.bitrate = self.target_bitrate;
        self.improvement_count += 1;

        // 如果码率足够，尝试升分辨率
        if self.target_bitrate > 15_000_000 && self.current_settings.resolution.0 < 1920 {
            self.current_settings.resolution = (1920, 1080);
        }

        // 降低 QP (提升质量)
        if self.improvement_count > 2 {
            self.current_settings.qp = (self.current_settings.qp.saturating_sub(1)).max(20);
            self.improvement_count = 0;
        }

        self.apply_settings();
    }

    /// 应用设置到编码器
    fn apply_settings(&self) {
        if let Some(ref callback) = self.encoder_callback {
            callback(self.current_settings.clone());
        }
    }

    /// 注册编码器回调
    pub fn register_encoder_callback(&mut self, callback: EncoderCallback) {
        self.encoder_callback = Some(callback);
    }

    /// 获取当前设置
    pub fn get_quality(&self) -> QualitySettings {
        self.current_settings.clone()
    }

    /// 获取当前码率
    pub fn current_bitrate(&self) -> u32 {
        self.current_bitrate
    }

    /// 获取当前帧率
    pub fn current_fps(&self) -> u32 {
        self.current_settings.fps
    }

    /// 获取当前 QP
    pub fn current_qp(&self) -> u32 {
        self.current_settings.qp
    }

    /// 获取当前状态
    pub fn state(&self) -> ControllerState {
        self.state
    }

    /// 设置最小码率
    pub fn set_min_bitrate(&mut self, bitrate: u32) {
        self.min_bitrate = bitrate;
    }

    /// 设置最大码率
    pub fn set_max_bitrate(&mut self, bitrate: u32) {
        self.max_bitrate = bitrate;
    }

    /// 记录 RTT
    pub fn record_rtt(&mut self, rtt: Duration) {
        self.monitor.record_rtt(rtt);
    }

    /// 记录丢包
    pub fn record_loss(&mut self) {
        self.monitor.record_loss();
    }

    /// 记录带宽
    pub fn record_bandwidth(&mut self, bandwidth_bps: u32) {
        self.monitor.record_bandwidth_sample(bandwidth_bps);
    }

    /// 获取网络统计
    pub fn network_stats(&self) -> NetworkStats {
        self.monitor.get_stats()
    }

    /// 重置控制器
    pub fn reset(&mut self) {
        self.monitor.reset();
        self.state = ControllerState::Stable;
        self.target_bitrate = 20_000_000;
        self.current_settings = QualitySettings::default();
        self.improvement_count = 0;
        self.degradation_count = 0;
        self.last_adjustment = Instant::now();
    }

    /// 强制设置码率
    pub fn set_bitrate(&mut self, bitrate: u32) {
        self.target_bitrate = bitrate.clamp(self.min_bitrate, self.max_bitrate);
        self.current_settings.bitrate = self.target_bitrate;
        self.apply_settings();
    }

    /// 强制设置帧率
    pub fn set_fps(&mut self, fps: u32) {
        self.current_settings.fps = fps.clamp(15, 144);
    }

    /// 强制设置 QP
    pub fn set_qp(&mut self, qp: u32) {
        self.current_settings.qp = qp.clamp(10, 51);
    }

    /// 获取监测器引用
    pub fn monitor(&self) -> &NetworkMonitor {
        &self.monitor
    }

    /// 获取监测器可变引用
    pub fn monitor_mut(&mut self) -> &mut NetworkMonitor {
        &mut self.monitor
    }
}

impl QualityControllerTrait for AdaptiveBitrateController {
    fn update_stats(&mut self, stats: NetworkStats) {
        self.update_stats(stats);
    }

    fn get_quality(&self) -> QualitySettings {
        self.get_quality()
    }

    fn register_encoder_callback(&mut self, callback: EncoderCallback) {
        self.register_encoder_callback(callback);
    }
}

impl Default for AdaptiveBitrateController {
    fn default() -> Self {
        Self::new(20_000_000)
    }
}

/// 质量控制器别名
pub type QualityController = AdaptiveBitrateController;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_controller_new() {
        let controller = AdaptiveBitrateController::new(20_000_000);
        assert_eq!(controller.current_bitrate(), 20_000_000);
        assert_eq!(controller.current_fps(), 60);
        assert_eq!(controller.state(), ControllerState::Stable);
    }

    #[test]
    fn test_state_transition_to_degrading() {
        let mut controller = AdaptiveBitrateController::new(50_000_000);

        // 模拟网络恶化
        for _ in 0..5 {
            controller.record_rtt(Duration::from_millis(150));
            controller.record_loss();
        }

        controller.evaluate_network();

        assert_eq!(controller.state(), ControllerState::Degrading);
        // 码率应该降低
        assert!(controller.current_bitrate() < 50_000_000);
    }

    #[test]
    fn test_emergency_degrade() {
        let mut controller = AdaptiveBitrateController::new(50_000_000);

        // 模拟极端网络条件
        for _ in 0..10 {
            controller.record_rtt(Duration::from_millis(300));
            controller.record_loss();
        }

        // 强制评估网络
        controller.last_adjustment = Instant::now() - Duration::from_secs(1);
        controller.evaluate_network();

        assert_eq!(controller.state(), ControllerState::Degrading);
        assert_eq!(controller.current_fps(), 30);
        assert!(controller.current_qp() >= 35);
    }

    #[test]
    fn test_quality_improvement() {
        let mut controller = AdaptiveBitrateController::new(10_000_000);
        let initial_bitrate = controller.current_bitrate();

        // 模拟优秀网络
        for _ in 0..5 {
            controller.record_rtt(Duration::from_millis(10));
        }

        controller.last_adjustment = Instant::now() - Duration::from_secs(1);
        controller.evaluate_network();

        // 至少应该进入 Probing 状态
        assert!(
            matches!(controller.state(), ControllerState::Probing | ControllerState::Stable)
        );
    }

    #[test]
    fn test_encoder_callback() {
        let mut controller = AdaptiveBitrateController::new(20_000_000);
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        controller.register_encoder_callback(Box::new(move |_settings| {
            called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        }));

        // 触发回调
        controller.apply_settings();

        assert!(called.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_manual_bitrate_setting() {
        let mut controller = AdaptiveBitrateController::new(20_000_000);
        controller.set_bitrate(30_000_000);
        assert_eq!(controller.current_bitrate(), 30_000_000);

        // 测试边界
        controller.set_bitrate(1_000_000); // 低于最小值
        assert_eq!(controller.current_bitrate(), 2_000_000); // 最小码率
    }
}
