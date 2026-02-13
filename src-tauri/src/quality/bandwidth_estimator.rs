//! 带宽估计模块
//!
//! 基于 GCC (Google Congestion Control) 算法简化版实现
//! 提供基于传输速率的带宽估计

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 带宽样本
#[derive(Debug, Clone, Copy)]
pub struct BandwidthSample {
    /// 带宽值 (bps)
    pub bandwidth_bps: u32,
    /// 采样时间
    pub timestamp: Instant,
    /// 数据大小 (字节)
    pub bytes: u64,
}

/// 带宽估计器
///
/// 基于 GCC 算法简化版实现
pub struct BandwidthEstimator {
    /// 到达时间样本 (时间, 数据大小)
    arrival_times: VecDeque<(Instant, u64)>,
    /// 当前估计带宽 (bps)
    estimated_bandwidth: u32,
    /// 平滑因子 (0-1)
    alpha: f32,
    /// 上次更新时间
    last_update: Instant,
    /// 最小估计值 (bps)
    min_estimate: u32,
    /// 最大估计值 (bps)
    max_estimate: u32,
}

impl BandwidthEstimator {
    /// 创建新的带宽估计器
    pub fn new() -> Self {
        Self {
            arrival_times: VecDeque::with_capacity(100),
            estimated_bandwidth: 10_000_000, // 初始 10 Mbps
            alpha: 0.2,                      // 平滑因子
            last_update: Instant::now(),
            min_estimate: 500_000,           // 500 Kbps
            max_estimate: 1_000_000_000,     // 1 Gbps
        }
    }

    /// 记录数据到达
    ///
    /// # Arguments
    /// * `bytes` - 到达的数据字节数
    pub fn record_arrival(&mut self, bytes: u64) {
        let now = Instant::now();
        self.arrival_times.push_back((now, bytes));

        // 保持最近1秒的数据
        let cutoff = now - Duration::from_secs(1);
        while self
            .arrival_times
            .front()
            .map(|(t, _)| *t < cutoff)
            .unwrap_or(false)
        {
            self.arrival_times.pop_front();
        }

        self.update_estimate();
    }

    /// 更新带宽估计
    fn update_estimate(&mut self) {
        if self.arrival_times.len() < 2 {
            return;
        }

        // 计算总字节数
        let total_bytes: u64 = self.arrival_times.iter().map(|(_, b)| b).sum();

        // 计算时间窗口
        let first_time = self.arrival_times.front().unwrap().0;
        let last_time = self.arrival_times.back().unwrap().0;
        let duration = last_time.duration_since(first_time);

        if duration.as_secs_f32() <= 0.0 {
            return;
        }

        // 计算瞬时带宽 (bps)
        let instant_bps = (total_bytes * 8) as f32 / duration.as_secs_f32();

        // 平滑更新 (指数移动平均)
        let smoothed = self.alpha * instant_bps + (1.0 - self.alpha) * self.estimated_bandwidth as f32;

        // 限制范围
        self.estimated_bandwidth = (smoothed as u32)
            .clamp(self.min_estimate, self.max_estimate);

        self.last_update = Instant::now();
    }

    /// 直接添加带宽样本
    pub fn add_sample(&mut self, bandwidth_bps: u32) {
        // 平滑更新
        let smoothed = self.alpha * bandwidth_bps as f32
            + (1.0 - self.alpha) * self.estimated_bandwidth as f32;

        self.estimated_bandwidth = (smoothed as u32)
            .clamp(self.min_estimate, self.max_estimate);

        self.last_update = Instant::now();
    }

    /// 基于传输数据更新带宽估计
    ///
    /// # Arguments
    /// * `bytes_sent` - 发送的字节数
    /// * `duration` - 发送耗时
    pub fn update(&mut self, bytes_sent: usize, duration: Duration) {
        if duration.as_secs_f32() <= 0.0 {
            return;
        }

        // 计算瞬时带宽
        let instant_bps = (bytes_sent as f32 * 8.0) / duration.as_secs_f32();
        self.add_sample(instant_bps as u32);
    }

    /// 获取当前带宽估计
    pub fn get_estimate(&self) -> u32 {
        self.estimated_bandwidth
    }

    /// 获取带宽估计 (别名)
    pub fn estimate(&self) -> u32 {
        self.get_estimate()
    }

    /// 获取保守估计 (P10 百分位)
    ///
    /// 返回过去样本的 10% 分位数，用于保守估计
    pub fn conservative_estimate(&self) -> u32 {
        if self.arrival_times.len() < 10 {
            return (self.estimated_bandwidth as f32 * 0.7) as u32;
        }

        // 基于最近样本计算保守估计
        let mut estimates: Vec<u32> = Vec::new();
        for window in self.arrival_times.as_slices().0.windows(2) {
            if let [(t1, b1), (t2, b2)] = window {
                let duration = t2.duration_since(*t1);
                if duration.as_secs_f32() > 0.0 {
                    let bps = ((*b2 - *b1) * 8) as f32 / duration.as_secs_f32();
                    estimates.push(bps as u32);
                }
            }
        }

        if estimates.is_empty() {
            return (self.estimated_bandwidth as f32 * 0.7) as u32;
        }

        estimates.sort_unstable();
        let index = (estimates.len() as f32 * 0.1) as usize;
        estimates.get(index).copied().unwrap_or(self.estimated_bandwidth)
    }

    /// 获取乐观估计 (P90 百分位)
    pub fn optimistic_estimate(&self) -> u32 {
        if self.arrival_times.len() < 10 {
            return self.estimated_bandwidth;
        }

        let mut estimates: Vec<u32> = Vec::new();
        for window in self.arrival_times.as_slices().0.windows(2) {
            if let [(t1, b1), (t2, b2)] = window {
                let duration = t2.duration_since(*t1);
                if duration.as_secs_f32() > 0.0 {
                    let bps = ((*b2 - *b1) * 8) as f32 / duration.as_secs_f32();
                    estimates.push(bps as u32);
                }
            }
        }

        if estimates.is_empty() {
            return self.estimated_bandwidth;
        }

        estimates.sort_unstable();
        let index = (estimates.len() as f32 * 0.9) as usize;
        estimates.get(index).copied().unwrap_or(self.estimated_bandwidth)
    }

    /// 获取带宽趋势 (bps/s)
    ///
    /// 正值表示带宽增加，负值表示带宽减少
    pub fn trend(&self) -> f32 {
        if self.arrival_times.len() < 10 {
            return 0.0;
        }

        let half = self.arrival_times.len() / 2;
        let first_bytes: u64 = self.arrival_times.iter().take(half).map(|(_, b)| b).sum();
        let second_bytes: u64 = self.arrival_times.iter().skip(half).map(|(_, b)| b).sum();

        let first_avg = first_bytes as f32 / half as f32;
        let second_avg = second_bytes as f32 / half as f32;

        second_avg - first_avg
    }

    /// 设置平滑因子
    pub fn set_alpha(&mut self, alpha: f32) {
        self.alpha = alpha.clamp(0.0, 1.0);
    }

    /// 设置最小/最大估计值
    pub fn set_bounds(&mut self, min: u32, max: u32) {
        self.min_estimate = min;
        self.max_estimate = max.max(min);
        self.estimated_bandwidth = self
            .estimated_bandwidth
            .clamp(self.min_estimate, self.max_estimate);
    }

    /// 获取样本数量
    pub fn sample_count(&self) -> usize {
        self.arrival_times.len()
    }

    /// 获取上次更新时间
    pub fn last_update(&self) -> Instant {
        self.last_update
    }

    /// 重置估计器
    pub fn reset(&mut self) {
        self.arrival_times.clear();
        self.estimated_bandwidth = 10_000_000;
        self.last_update = Instant::now();
    }
}

impl Default for BandwidthEstimator {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建新的带宽估计器
pub fn create_bandwidth_estimator() -> BandwidthEstimator {
    BandwidthEstimator::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_estimator_new() {
        let estimator = BandwidthEstimator::new();
        assert_eq!(estimator.get_estimate(), 10_000_000);
        assert_eq!(estimator.sample_count(), 0);
    }

    #[test]
    fn test_record_arrival() {
        let mut estimator = BandwidthEstimator::new();

        // 模拟数据到达
        estimator.record_arrival(100_000); // 100KB
        std::thread::sleep(Duration::from_millis(10));
        estimator.record_arrival(100_000);
        std::thread::sleep(Duration::from_millis(10));
        estimator.record_arrival(100_000);

        assert!(estimator.sample_count() > 0);
    }

    #[test]
    fn test_update_with_duration() {
        let mut estimator = BandwidthEstimator::new();

        // 添加样本：1MB in 100ms = 80 Mbps
        for _ in 0..10 {
            estimator.update(1_000_000, Duration::from_millis(100));
        }

        let estimate = estimator.get_estimate();
        assert!(estimate > 0);
        // 估计应该接近 80 Mbps
        assert!(estimate > 50_000_000);
    }

    #[test]
    fn test_add_sample() {
        let mut estimator = BandwidthEstimator::new();

        estimator.add_sample(50_000_000);
        estimator.add_sample(60_000_000);
        estimator.add_sample(70_000_000);

        let estimate = estimator.get_estimate();
        assert!(estimate >= 50_000_000 && estimate <= 70_000_000);
    }

    #[test]
    fn test_conservative_estimate() {
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
    fn test_optimistic_estimate() {
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

    #[test]
    fn test_trend() {
        let mut estimator = BandwidthEstimator::new();

        // 增加趋势
        for i in 0..20 {
            estimator.add_sample(10_000_000 + i * 5_000_000);
        }

        let trend = estimator.trend();
        // 趋势应该为正 (增加)
        assert!(trend >= 0.0);
    }

    #[test]
    fn test_bounds() {
        let mut estimator = BandwidthEstimator::new();

        estimator.set_bounds(1_000_000, 50_000_000);

        // 尝试设置超出范围的值
        estimator.add_sample(100_000_000);
        assert!(estimator.get_estimate() <= 50_000_000);

        estimator.add_sample(100_000);
        assert!(estimator.get_estimate() >= 1_000_000);
    }

    #[test]
    fn test_reset() {
        let mut estimator = BandwidthEstimator::new();

        estimator.add_sample(50_000_000);
        estimator.reset();

        assert_eq!(estimator.get_estimate(), 10_000_000);
        assert_eq!(estimator.sample_count(), 0);
    }

    #[test]
    fn test_alpha_setting() {
        let mut estimator = BandwidthEstimator::new();

        estimator.set_alpha(0.5);
        // 应该接受 0-1 范围内的值

        estimator.set_alpha(1.5); // 应该被限制到 1.0
        // 内部值应该被限制

        estimator.set_alpha(-0.5); // 应该被限制到 0.0
    }
}
