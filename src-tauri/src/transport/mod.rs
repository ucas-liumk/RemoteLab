//! RemoteLab Ultra - Transport Layer
//! 
//! 提供多种传输协议支持：
//! - QUIC: 低延迟视频传输，支持不可靠数据报
//! - WebTransport: 浏览器兼容的现代 Web 传输
//! - SSH Tunnel: 传统回退方案

use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod protocol;
pub mod quic;
pub mod webtransport;
pub mod sshtunnel;

#[cfg(test)]
mod tests;

pub use protocol::*;

/// 传输层 trait - 定义所有传输协议的公共接口
#[async_trait]
pub trait Transport: Send + Sync {
    /// 连接到服务器
    /// 
    /// # Arguments
    /// * `addr` - 服务器地址
    /// 
    /// # Returns
    /// * `Ok(())` - 连接成功
    /// * `Err(TransportError)` - 连接失败
    async fn connect(&mut self, addr: SocketAddr) -> Result<(), TransportError>;

    /// 发送视频帧（使用不可靠传输，低延迟）
    /// 
    /// 视频数据使用 datagram 方式发送，允许丢包以换取低延迟
    /// 
    /// # Arguments
    /// * `frame` - 编码后的视频帧
    async fn send_video(&self, frame: &EncodedFrame) -> Result<(), TransportError>;

    /// 发送输入事件（使用可靠传输）
    /// 
    /// 输入事件使用可靠流传输，确保不丢失
    /// 
    /// # Arguments
    /// * `event` - 输入事件
    async fn send_input(&self, event: InputEvent) -> Result<(), TransportError>;

    /// 接收视频数据
    /// 
    /// 阻塞等待直到收到视频包或连接断开
    async fn recv_video(&self) -> Result<VideoPacket, TransportError>;

    /// 接收输入确认
    /// 
    /// 接收服务器对输入事件的确认
    async fn recv_input_ack(&self) -> Result<InputAck, TransportError>;

    /// 发送控制包
    /// 
    /// 用于发送心跳、带宽探测等控制消息
    async fn send_control(&self, packet: ControlPacket) -> Result<(), TransportError>;

    /// 接收控制包
    async fn recv_control(&self) -> Result<ControlPacket, TransportError>;

    /// 获取网络统计信息
    /// 
    /// 返回实时的网络质量指标
    fn stats(&self) -> NetworkStats;

    /// 是否已连接
    fn is_connected(&self) -> bool;

    /// 断开连接
    /// 
    /// 优雅地关闭连接，释放资源
    async fn disconnect(&mut self);

    /// 获取传输模式
    fn mode(&self) -> TransportMode;
}

/// 传输层工厂
/// 
/// 根据配置创建对应的传输实例
pub struct TransportFactory;

impl TransportFactory {
    /// 创建传输实例
    /// 
    /// # Arguments
    /// * `config` - 连接配置
    /// 
    /// # Returns
    /// 对应传输模式的实例
    pub fn create(config: &ConnectionConfig) -> Result<Box<dyn Transport>, TransportError> {
        match config.mode {
            TransportMode::Quic => {
                let transport = quic::QuicTransport::new(config.clone())?;
                Ok(Box::new(transport))
            }
            TransportMode::WebTransport => {
                let transport = webtransport::WebTransportClient::new(config.clone())?;
                Ok(Box::new(transport))
            }
            TransportMode::SshTunnel => {
                let transport = sshtunnel::SshTunnelTransport::new(config.clone())?;
                Ok(Box::new(transport))
            }
        }
    }

    /// 创建 QUIC 传输
    pub fn create_quic(config: ConnectionConfig) -> Result<quic::QuicTransport, TransportError> {
        quic::QuicTransport::new(config)
    }

    /// 创建 WebTransport 传输
    pub fn create_webtransport(
        config: ConnectionConfig,
    ) -> Result<webtransport::WebTransportClient, TransportError> {
        webtransport::WebTransportClient::new(config)
    }

    /// 创建 SSH 隧道传输
    pub fn create_sshtunnel(
        config: ConnectionConfig,
    ) -> Result<sshtunnel::SshTunnelTransport, TransportError> {
        sshtunnel::SshTunnelTransport::new(config)
    }
}

/// 传输层管理器
/// 
/// 管理多个传输连接，支持自动重连和负载均衡
pub struct TransportManager {
    transports: Vec<Arc<Mutex<Box<dyn Transport>>>>,
    primary_index: usize,
    config: ConnectionConfig,
}

impl TransportManager {
    /// 创建新的传输管理器
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            transports: Vec::new(),
            primary_index: 0,
            config,
        }
    }

    /// 添加传输实例
    pub fn add_transport(&mut self, transport: Box<dyn Transport>) {
        self.transports.push(Arc::new(Mutex::new(transport)));
    }

    /// 获取主传输
    pub fn primary(&self) -> Option<Arc<Mutex<Box<dyn Transport>>>> {
        self.transports.get(self.primary_index).cloned()
    }

    /// 切换到备用传输
    pub fn failover(&mut self) -> bool {
        if self.transports.len() > 1 {
            self.primary_index = (self.primary_index + 1) % self.transports.len();
            true
        } else {
            false
        }
    }

    /// 获取最佳传输（基于网络质量）
    pub fn best_transport(&self) -> Option<Arc<Mutex<Box<dyn Transport>>>> {
        let mut best_idx = 0;
        let mut best_score = f32::MAX;

        for (idx, transport) in self.transports.iter().enumerate() {
            if let Ok(t) = transport.try_lock() {
                let stats = t.stats();
                // 评分：RTT + 丢包率 * 100 + 抖动
                let score = stats.rtt_ms + stats.loss_rate * 100.0 + stats.jitter_ms;
                if score < best_score {
                    best_score = score;
                    best_idx = idx;
                }
            }
        }

        self.transports.get(best_idx).cloned()
    }

    /// 获取所有传输的统计信息
    pub fn all_stats(&self) -> Vec<(TransportMode, NetworkStats)> {
        self.transports
            .iter()
            .filter_map(|t| {
                if let Ok(transport) = t.try_lock() {
                    Some((transport.mode(), transport.stats()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// 断开所有连接
    pub async fn disconnect_all(&mut self) {
        for transport in &self.transports {
            if let Ok(mut t) = transport.try_lock() {
                t.disconnect().await;
            }
        }
        self.transports.clear();
    }
}

/// 延迟测量工具
pub struct LatencyMonitor {
    samples: Vec<f32>,
    max_samples: usize,
}

impl LatencyMonitor {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    /// 添加样本
    pub fn add_sample(&mut self, rtt_ms: f32) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(rtt_ms);
    }

    /// 计算平均 RTT
    pub fn average_rtt(&self) -> f32 {
        if self.samples.is_empty() {
            0.0
        } else {
            self.samples.iter().sum::<f32>() / self.samples.len() as f32
        }
    }

    /// 计算抖动（标准差）
    pub fn jitter(&self) -> f32 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let avg = self.average_rtt();
        let variance: f32 = self
            .samples
            .iter()
            .map(|&x| (x - avg).powi(2))
            .sum::<f32>()
            / self.samples.len() as f32;
        variance.sqrt()
    }

    /// 计算丢包率（基于序列号间隙）
    pub fn loss_rate(&self, expected: u32, received: u32) -> f32 {
        if expected == 0 {
            return 0.0;
        }
        1.0 - (received as f32 / expected as f32)
    }
}

/// 带宽估计器
pub struct BandwidthEstimator {
    /// 上一个样本时间
    last_time: std::time::Instant,
    /// 上一个样本的字节数
    last_bytes: u64,
    /// 带宽估计值（bps）
    estimate_bps: u32,
    /// 平滑因子
    alpha: f32,
}

impl BandwidthEstimator {
    pub fn new() -> Self {
        Self {
            last_time: std::time::Instant::now(),
            last_bytes: 0,
            estimate_bps: 10_000_000, // 初始估计 10 Mbps
            alpha: 0.8,
        }
    }

    /// 更新带宽估计
    pub fn update(&mut self, total_bytes: u64) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_time).as_secs_f32();

        if elapsed > 0.0 && total_bytes > self.last_bytes {
            let bytes_delta = total_bytes - self.last_bytes;
            let instant_bps = (bytes_delta as f32 * 8.0) / elapsed;

            // 指数移动平均
            self.estimate_bps =
                (self.alpha * self.estimate_bps as f32 + (1.0 - self.alpha) * instant_bps) as u32;

            self.last_time = now;
            self.last_bytes = total_bytes;
        }
    }

    pub fn estimate(&self) -> u32 {
        self.estimate_bps
    }
}

impl Default for BandwidthEstimator {
    fn default() -> Self {
        Self::new()
    }
}
