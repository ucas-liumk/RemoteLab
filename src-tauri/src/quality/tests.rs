//! 质量模块测试
//!
//! 包含完整的单元测试和集成测试

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::{
    bandwidth_estimator::BandwidthEstimator,
    monitor::NetworkMonitor,
    adaptive_controller::{AdaptiveBitrateController, ControllerState, QualityController},
    CodecType, NetworkStats, QualitySettings, QualityControllerTrait,
};

/// 测试辅助函数：创建模拟网络条件
fn simulate_network_condition(
    monitor: &mut NetworkMonitor,
    rtt_ms: Vec<u64>,
    loss_count: u32,
) {
    for rtt in rtt_ms {
        monitor.record_rtt(Duration::from_millis(rtt));
    }

    for _ in 0..loss_count {
        monitor.record_loss();
    }
}

// ============================================================================
// Network Monitor Tests
// ============================================================================

#[test]
fn test_network_monitor_rtt_collection() {
    let mut monitor = NetworkMonitor::new();

    // 记录 RTT 样本
    monitor.record_rtt(Duration::from_millis(10));
    monitor.record_rtt(Duration::from_millis(20));
    monitor.record_rtt(Duration::from_millis(30));

    let stats = monitor.get_stats();
    assert_eq!(stats.rtt_ms, 20.0);
}

#[test]
fn test_network_monitor_loss_rate() {
    let mut monitor = NetworkMonitor::new();

    // 模拟 10 个包，2 个丢失
    for _ in 0..8 {
        monitor.record_success(1000, Duration::from_millis(10));
    }
    for _ in 0..2 {
        monitor.record_loss();
    }

    let stats = monitor.get_stats();
    assert!((stats.loss_rate - 0.2).abs() < 0.01);
}

#[test]
fn test_network_monitor_jitter() {
    let mut monitor = NetworkMonitor::new();

    // 记录变化较大的 RTT，产生抖动
    monitor.record_rtt(Duration::from_millis(10));
    monitor.record_rtt(Duration::from_millis(50));
    monitor.record_rtt(Duration::from_millis(10));
    monitor.record_rtt(Duration::from_millis(50));

    let stats = monitor.get_stats();
    assert!(stats.jitter_ms > 0.0);
}

#[test]
fn test_packet_record() {
    let mut monitor = NetworkMonitor::new();
    let send_time = std::time::Instant::now() - Duration::from_millis(20);

    monitor.record_packet(1, send_time);

    let stats = monitor.get_stats();
    // RTT 应该接近 20ms (允许一定误差)
    assert!(stats.rtt_ms >= 15.0 && stats.rtt_ms <= 50.0);
}

#[test]
fn test_network_monitor_percentiles() {
    let mut monitor = NetworkMonitor::new();

    // 添加 100 个样本，从 1ms 到 100ms
    for i in 1..=100 {
        monitor.record_rtt(Duration::from_millis(i));
    }

    let p95 = monitor.p95_rtt();
    let p99 = monitor.p99_rtt();

    // P95 应该接近 95ms，P99 应该接近 99ms
    assert!(p95 >= 90.0 && p95 <= 100.0);
    assert!(p99 >= 95.0 && p99 <= 100.0);
}

// ============================================================================
// Adaptive Bitrate Controller Tests
// ============================================================================

#[test]
fn test_adaptive_controller_new() {
    let controller = AdaptiveBitrateController::new(20_000_000);
    assert_eq!(controller.current_bitrate(), 20_000_000);
    assert_eq!(controller.current_fps(), 60);
    assert_eq!(controller.current_qp(), 25);
    assert_eq!(controller.state(), ControllerState::Stable);
}

#[test]
fn test_state_transition_to_degrading() {
    let mut controller = AdaptiveBitrateController::new(50_000_000);
    let initial_bitrate = controller.current_bitrate();

    // 模拟网络恶化
    for _ in 0..5 {
        controller.record_rtt(Duration::from_millis(150));
        controller.record_loss();
    }

    // 使用 update_stats 触发评估
    let stats = NetworkStats {
        rtt_ms: 150.0,
        jitter_ms: 10.0,
        loss_rate: 0.05,
        bandwidth_bps: 5_000_000,
        timestamp: std::time::Instant::now(),
    };

    // 强制设置上次调整时间为过去，确保会触发调整
    controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);
    controller.update_stats(stats);

    assert_eq!(controller.state(), ControllerState::Degrading);
    // 码率应该降低
    assert!(controller.current_bitrate() < initial_bitrate);
}

#[test]
fn test_emergency_degrade() {
    let mut controller = AdaptiveBitrateController::new(50_000_000);

    // 模拟极端网络条件
    for _ in 0..10 {
        controller.record_rtt(Duration::from_millis(300));
        controller.record_loss();
    }

    let stats = NetworkStats {
        rtt_ms: 300.0,
        jitter_ms: 50.0,
        loss_rate: 0.1,
        bandwidth_bps: 1_000_000,
        timestamp: std::time::Instant::now(),
    };

    controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);
    controller.update_stats(stats);

    assert_eq!(controller.state(), ControllerState::Degrading);
    // 帧率应该降到 30
    assert_eq!(controller.current_fps(), 30);
    // QP 应该增加
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

    let stats = NetworkStats {
        rtt_ms: 10.0,
        jitter_ms: 2.0,
        loss_rate: 0.0005,
        bandwidth_bps: 100_000_000,
        timestamp: std::time::Instant::now(),
    };

    controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);
    controller.update_stats(stats);

    // 应该进入 Probing 或保持 Stable
    assert!(
        matches!(controller.state(), ControllerState::Probing | ControllerState::Stable)
    );
}

#[test]
fn test_encoder_callback() {
    let mut controller = AdaptiveBitrateController::new(20_000_000);
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_clone = called.clone();

    controller.register_encoder_callback(Box::new(move |settings| {
        assert_eq!(settings.bitrate, 20_000_000);
        called_clone.store(true, Ordering::Relaxed);
    }));

    // 触发回调
    let quality = controller.get_quality();
    // 手动触发回调
    if let Some(ref callback) = controller.encoder_callback {
        callback(quality);
    }

    assert!(called.load(Ordering::Relaxed));
}

#[test]
fn test_manual_bitrate_setting() {
    let mut controller = AdaptiveBitrateController::new(20_000_000);
    controller.set_bitrate(30_000_000);
    assert_eq!(controller.current_bitrate(), 30_000_000);

    // 测试边界
    controller.set_bitrate(100_000); // 低于最小值
    assert!(controller.current_bitrate() >= 2_000_000); // 最小码率
}

#[test]
fn test_quality_controller_trait() {
    let mut controller = AdaptiveBitrateController::new(20_000_000);

    // 测试 trait 方法
    let stats = NetworkStats {
        rtt_ms: 50.0,
        jitter_ms: 5.0,
        loss_rate: 0.01,
        bandwidth_bps: 50_000_000,
        timestamp: std::time::Instant::now(),
    };

    QualityControllerTrait::update_stats(&mut controller, stats);

    let quality = QualityControllerTrait::get_quality(&controller);
    assert!(quality.bitrate > 0);
}

// ============================================================================
// Bandwidth Estimator Tests
// ============================================================================

#[test]
fn test_bandwidth_estimator_basic() {
    let mut estimator = BandwidthEstimator::new();

    // 添加样本：1MB in 100ms = 80 Mbps
    for _ in 0..10 {
        estimator.update(1_000_000, Duration::from_millis(100));
    }

    let estimate = estimator.get_estimate();
    assert!(estimate > 0);
}

#[test]
fn test_bandwidth_record_arrival() {
    let mut estimator = BandwidthEstimator::new();

    // 模拟数据到达
    estimator.record_arrival(100_000);
    std::thread::sleep(Duration::from_millis(5));
    estimator.record_arrival(100_000);
    std::thread::sleep(Duration::from_millis(5));
    estimator.record_arrival(100_000);

    assert!(estimator.sample_count() > 0);
}

#[test]
fn test_bandwidth_conservative_estimate() {
    let mut estimator = BandwidthEstimator::new();

    // 添加不同带宽的样本
    for i in 0..20 {
        let bandwidth = 10_000_000 + i * 1_000_000;
        estimator.add_sample(bandwidth);
    }

    let estimate = estimator.get_estimate();
    let conservative = estimator.conservative_estimate();

    // 保守估计应该小于等于实际估计
    assert!(conservative <= estimate);
}

#[test]
fn test_bandwidth_optimistic_estimate() {
    let mut estimator = BandwidthEstimator::new();

    for i in 0..20 {
        let bandwidth = 10_000_000 + i * 1_000_000;
        estimator.add_sample(bandwidth);
    }

    let estimate = estimator.get_estimate();
    let optimistic = estimator.optimistic_estimate();

    // 乐观估计应该大于等于实际估计
    assert!(optimistic >= estimate);
}

// ============================================================================
// Integration Tests
// ============================================================================

/// 集成测试：完整的质量控制流程
#[test]
fn test_full_quality_control_workflow() {
    // 1. 创建网络监测器
    let mut monitor = NetworkMonitor::new();

    // 2. 模拟网络从好变差再到恢复
    // 良好网络
    for i in 0..10 {
        monitor.record_rtt(Duration::from_millis(10 + i as u64));
        monitor.record_success(10000, Duration::from_millis(10));
    }

    let stats1 = monitor.get_stats();
    assert!(stats1.rtt_ms < 25.0);

    // 网络恶化
    for _ in 0..10 {
        monitor.record_rtt(Duration::from_millis(200));
        monitor.record_loss();
    }

    let stats2 = monitor.get_stats();
    assert!(stats2.loss_rate > 0.3); // 至少 30% 丢包

    // 3. 创建自适应控制器
    let mut controller = AdaptiveBitrateController::new(20_000_000);

    // 4. 根据监测器状态调整
    for _ in 0..3 {
        let stats = monitor.get_stats();
        controller.update_stats(stats);
        controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);
    }

    // 验证调整合理性
    let final_quality = controller.get_quality();
    assert!(final_quality.bitrate >= 2_000_000);
    assert!(final_quality.fps >= 15 && final_quality.fps <= 144);
    assert!(final_quality.qp >= 10 && final_quality.qp <= 51);
}

/// 压力测试：大量样本处理
#[test]
fn test_stress_many_samples() {
    let mut monitor = NetworkMonitor::new();
    let mut controller = AdaptiveBitrateController::new(20_000_000);

    // 处理 1000 个样本
    for i in 0..1000 {
        let rtt = 20 + (i % 50) as u64; // 20-70ms 变化
        monitor.record_rtt(Duration::from_millis(rtt));
        controller.record_rtt(Duration::from_millis(rtt));

        if i % 10 == 0 {
            controller.record_loss();
        }
    }

    // 验证状态正确
    let stats = monitor.get_stats();
    assert!(stats.rtt_ms > 0.0);

    // 带宽估计器测试
    let mut estimator = BandwidthEstimator::new();
    for i in 0..1000 {
        let bandwidth = 10_000_000 + (i % 50) * 1_000_000;
        estimator.add_sample(bandwidth);
    }

    assert!(estimator.get_estimate() > 0);
}

/// 测试不同网络场景下的行为
#[test]
fn test_various_network_scenarios() {
    // 场景 1: 优秀网络 (局域网)
    {
        let mut controller = AdaptiveBitrateController::new(10_000_000);
        let stats = NetworkStats {
            rtt_ms: 5.0,
            jitter_ms: 1.0,
            loss_rate: 0.0,
            bandwidth_bps: 1_000_000_000,
            timestamp: std::time::Instant::now(),
        };
        controller.update_stats(stats);
        controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);

        let quality = controller.get_quality();
        assert!(quality.fps >= 60);
    }

    // 场景 2: 良好网络 (WiFi)
    {
        let mut controller = AdaptiveBitrateController::new(10_000_000);
        let stats = NetworkStats {
            rtt_ms: 30.0,
            jitter_ms: 5.0,
            loss_rate: 0.005,
            bandwidth_bps: 100_000_000,
            timestamp: std::time::Instant::now(),
        };
        controller.update_stats(stats);
        controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);

        let quality = controller.get_quality();
        assert!(quality.bitrate >= 5_000_000);
    }

    // 场景 3: 差网络 (移动网络/拥塞)
    {
        let mut controller = AdaptiveBitrateController::new(20_000_000);
        let stats = NetworkStats {
            rtt_ms: 200.0,
            jitter_ms: 50.0,
            loss_rate: 0.1,
            bandwidth_bps: 5_000_000,
            timestamp: std::time::Instant::now(),
        };
        controller.last_adjustment = std::time::Instant::now() - Duration::from_secs(1);
        controller.update_stats(stats);

        let quality = controller.get_quality();
        assert_eq!(quality.fps, 30); // 帧率应该降低
        assert!(quality.bitrate <= 15_000_000); // 码率应该降低
    }
}

/// 测试 QualitySettings 默认值
#[test]
fn test_quality_settings_default() {
    let settings = QualitySettings::default();
    assert_eq!(settings.bitrate, 20_000_000);
    assert_eq!(settings.fps, 60);
    assert_eq!(settings.qp, 28);
    assert_eq!(settings.resolution, (1920, 1080));
    assert_eq!(settings.codec, CodecType::H264);
}

/// 测试编解码器类型
#[test]
fn test_codec_type() {
    assert_eq!(CodecType::H264.as_str(), "H264");
    assert_eq!(CodecType::HEVC.as_str(), "HEVC");
    assert_eq!(CodecType::AV1.as_str(), "AV1");
}

/// 测试控制器重置
#[test]
fn test_controller_reset() {
    let mut controller = AdaptiveBitrateController::new(50_000_000);

    // 修改一些值
    controller.set_bitrate(10_000_000);
    controller.set_fps(30);
    controller.set_qp(40);

    // 重置
    controller.reset();

    assert_eq!(controller.current_bitrate(), 20_000_000);
    assert_eq!(controller.current_fps(), 60);
    assert_eq!(controller.state(), ControllerState::Stable);
}

/// 测试网络状态更新
#[test]
fn test_network_stats_update() {
    let mut controller = AdaptiveBitrateController::new(20_000_000);

    // 更新网络统计
    for i in 0..10 {
        let stats = NetworkStats {
            rtt_ms: 20.0 + i as f32 * 5.0,
            jitter_ms: i as f32,
            loss_rate: i as f32 * 0.001,
            bandwidth_bps: 50_000_000,
            timestamp: std::time::Instant::now(),
        };
        controller.update_stats(stats);
    }

    let final_stats = controller.network_stats();
    assert!(final_stats.rtt_ms > 0.0);
}

/// 测试带宽估计器的重置
#[test]
fn test_bandwidth_estimator_reset() {
    let mut estimator = BandwidthEstimator::new();

    estimator.add_sample(50_000_000);
    assert!(estimator.get_estimate() != 10_000_000);

    estimator.reset();
    assert_eq!(estimator.get_estimate(), 10_000_000);
    assert_eq!(estimator.sample_count(), 0);
}

/// 测试网络监测器重置
#[test]
fn test_network_monitor_reset() {
    let mut monitor = NetworkMonitor::new();

    monitor.record_rtt(Duration::from_millis(50));
    monitor.record_loss();
    monitor.record_bandwidth_sample(50_000_000);

    assert!(monitor.rtt_sample_count() > 0);
    assert!(monitor.loss_count() > 0);

    monitor.reset();

    assert_eq!(monitor.rtt_sample_count(), 0);
    assert_eq!(monitor.loss_count(), 0);
    assert_eq!(monitor.total_packets(), 0);
}
