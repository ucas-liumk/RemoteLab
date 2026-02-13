//! 网络质量监测模块
//!
//! 负责收集网络质量指标：RTT、丢包率、抖动、带宽估计

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::NetworkStats;

/// 网络质量监测器
pub struct NetworkMonitor {
    /// RTT 样本 (最后 100 个)
    rtt_samples: VecDeque<f32>,
    /// 抖动样本
    jitter_samples: VecDeque<f32>,
    /// 丢包计数
    loss_count: u32,
    /// 总包数
    total_packets: u32,
    /// 上次包到达时间 (用于计算抖动)
    last_packet_time: Option<Instant>,
    /// 带宽样本 (bps)
    bandwidth_samples: VecDeque<u32>,
}

impl NetworkMonitor {
    /// 创建新的网络监测器
    pub fn new() -> Self {
        Self {
            rtt_samples: VecDeque::with_capacity(100),
            jitter_samples: VecDeque::with_capacity(100),
            loss_count: 0,
            total_packets: 0,
            last_packet_time: None,
            bandwidth_samples: VecDeque::with_capacity(60),
        }
    }

    /// 记录数据包到达
    ///
    /// # Arguments
    /// * `seq` - 序列号 (用于检测丢包)
    /// * `timestamp` - 发送时间戳
    pub fn record_packet(&mut self, _seq: u32, timestamp: Instant) {
        let now = Instant::now();

        // 计算 RTT (毫秒)
        let rtt = now.duration_since(timestamp).as_millis() as f32;
        self.rtt_samples.push_back(rtt);

        // 计算抖动 (RFC 3550 风格 - 包到达间隔的变化)
        if let Some(last) = self.last_packet_time {
            let arrival_jitter = now.duration_since(last).as_millis() as f32;
            self.jitter_samples.push_back(arrival_jitter);
        }

        self.last_packet_time = Some(now);
        self.total_packets += 1;

        // 保持样本数量限制
        if self.rtt_samples.len() > 100 {
            self.rtt_samples.pop_front();
        }
        if self.jitter_samples.len() > 100 {
            self.jitter_samples.pop_front();
        }
    }

    /// 记录 RTT 样本 (直接)
    pub fn record_rtt(&mut self, rtt: Duration) {
        let rtt_ms = rtt.as_millis() as f32;
        self.rtt_samples.push_back(rtt_ms);

        // 计算抖动
        if self.rtt_samples.len() >= 2 {
            let prev = self.rtt_samples[self.rtt_samples.len() - 2];
            let jitter = (rtt_ms - prev).abs();
            self.jitter_samples.push_back(jitter);
        }

        if self.rtt_samples.len() > 100 {
            self.rtt_samples.pop_front();
        }
        if self.jitter_samples.len() > 100 {
            self.jitter_samples.pop_front();
        }
    }

    /// 记录丢包
    pub fn record_loss(&mut self) {
        self.loss_count += 1;
        self.total_packets += 1;
    }

    /// 记录成功传输
    pub fn record_success(&mut self, _bytes: usize, _duration: Duration) {
        self.total_packets += 1;
    }

    /// 记录带宽样本
    pub fn record_bandwidth_sample(&mut self, bandwidth_bps: u32) {
        self.bandwidth_samples.push_back(bandwidth_bps);
        if self.bandwidth_samples.len() > 60 {
            self.bandwidth_samples.pop_front();
        }
    }

    /// 获取网络统计
    pub fn get_stats(&self) -> NetworkStats {
        NetworkStats {
            rtt_ms: self.calculate_rtt(),
            jitter_ms: self.calculate_jitter(),
            loss_rate: self.calculate_loss_rate(),
            bandwidth_bps: self.estimate_bandwidth(),
            timestamp: Instant::now(),
        }
    }

    /// 计算平均 RTT
    fn calculate_rtt(&self) -> f32 {
        if self.rtt_samples.is_empty() {
            return 0.0;
        }
        self.rtt_samples.iter().sum::<f32>() / self.rtt_samples.len() as f32
    }

    /// 计算抖动 (RTT 方差的平方根)
    fn calculate_jitter(&self) -> f32 {
        if self.jitter_samples.len() < 2 {
            return 0.0;
        }

        let mean = self.jitter_samples.iter().sum::<f32>() / self.jitter_samples.len() as f32;
        let variance = self
            .jitter_samples
            .iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f32>()
            / self.jitter_samples.len() as f32;

        variance.sqrt()
    }

    /// 计算丢包率
    fn calculate_loss_rate(&self) -> f32 {
        if self.total_packets == 0 {
            return 0.0;
        }
        self.loss_count as f32 / self.total_packets as f32
    }

    /// 估计带宽
    fn estimate_bandwidth(&self) -> u32 {
        if self.bandwidth_samples.is_empty() {
            return 10_000_000; // 默认 10 Mbps
        }
        self.bandwidth_samples.iter().sum::<u32>() / self.bandwidth_samples.len() as u32
    }

    /// 获取当前 RTT 样本数
    pub fn rtt_sample_count(&self) -> usize {
        self.rtt_samples.len()
    }

    /// 获取当前丢包数
    pub fn loss_count(&self) -> u32 {
        self.loss_count
    }

    /// 获取总包数
    pub fn total_packets(&self) -> u32 {
        self.total_packets
    }

    /// 重置统计
    pub fn reset(&mut self) {
        self.rtt_samples.clear();
        self.jitter_samples.clear();
        self.bandwidth_samples.clear();
        self.loss_count = 0;
        self.total_packets = 0;
        self.last_packet_time = None;
    }

    /// 计算 P95 RTT
    pub fn p95_rtt(&self) -> f32 {
        if self.rtt_samples.is_empty() {
            return 0.0;
        }
        let mut samples: Vec<f32> = self.rtt_samples.iter().copied().collect();
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        super::percentile(&samples, 0.95)
    }

    /// 计算 P99 RTT
    pub fn p99_rtt(&self) -> f32 {
        if self.rtt_samples.is_empty() {
            return 0.0;
        }
        let mut samples: Vec<f32> = self.rtt_samples.iter().copied().collect();
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        super::percentile(&samples, 0.99)
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_monitor_basic() {
        let mut monitor = NetworkMonitor::new();

        // 记录一些 RTT 样本
        for i in 1..=10 {
            monitor.record_rtt(Duration::from_millis(i * 10));
        }

        let stats = monitor.get_stats();
        assert!(stats.rtt_ms > 0.0);
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
        let send_time = Instant::now() - Duration::from_millis(20);
        
        monitor.record_packet(1, send_time);
        
        let stats = monitor.get_stats();
        // RTT 应该接近 20ms (允许一定误差)
        assert!(stats.rtt_ms >= 15.0 && stats.rtt_ms <= 50.0);
    }
}
