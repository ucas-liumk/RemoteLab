//! SSH Tunnel Transport Implementation
//! 
//! 使用 SSH 隧道作为传输回退方案
//! 特点：
//! - 兼容性好，几乎所有服务器都支持 SSH
//! - 安全性高，自带加密
//! - 适合无法使用 QUIC/WebTransport 的环境

use super::protocol::*;
use super::Transport;
use async_trait::async_trait;
use bytes::BytesMut;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, Duration};

/// SSH 隧道传输实现
/// 
/// 注意：完整的 SSH 实现需要 libssh2 或 russh
/// 这里提供一个基于 TCP 的简化实现框架
pub struct SshTunnelTransport {
    /// 连接配置
    config: ConnectionConfig,
    /// 连接状态
    connected: Arc<RwLock<bool>>,
    /// 网络统计
    stats: Arc<RwLock<NetworkStats>>,
    /// 底层 TCP 流
    stream: Option<Arc<Mutex<TcpStream>>>,
    /// SSH 会话（如果使用 russh）
    #[cfg(feature = "russh")]
    ssh_session: Option<Arc<Mutex<russh::client::Handle<Client>>>>,
    /// 视频序列号
    video_seq: AtomicU32,
    /// 输入序列号
    input_seq: AtomicU64,
    /// 视频接收通道
    video_rx: Option<mpsc::UnboundedReceiver<VideoPacket>>,
    /// 输入确认接收通道
    input_ack_rx: Option<mpsc::UnboundedReceiver<InputAck>>,
    /// 控制包接收通道
    control_rx: Option<mpsc::UnboundedReceiver<ControlPacket>>,
    /// 关闭信号
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
    /// SSH 配置
    ssh_config: SshConfig,
}

/// SSH 配置
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// SSH 服务器地址
    pub ssh_host: String,
    /// SSH 端口
    pub ssh_port: u16,
    /// 用户名
    pub username: String,
    /// 密码（或使用密钥）
    pub password: Option<String>,
    /// 私钥路径
    pub private_key: Option<String>,
    /// 目标远程地址（通过隧道转发）
    pub remote_dest: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            ssh_host: "localhost".to_string(),
            ssh_port: 22,
            username: "user".to_string(),
            password: None,
            private_key: None,
            remote_dest: "localhost:8080".to_string(),
        }
    }
}

impl SshTunnelTransport {
    /// 创建新的 SSH 隧道传输
    pub fn new(config: ConnectionConfig) -> Result<Self, TransportError> {
        let ssh_config = SshConfig::default();
        Ok(Self {
            config,
            connected: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            stream: None,
            #[cfg(feature = "russh")]
            ssh_session: None,
            video_seq: AtomicU32::new(0),
            input_seq: AtomicU64::new(0),
            video_rx: None,
            input_ack_rx: None,
            control_rx: None,
            shutdown_tx: None,
            ssh_config,
        })
    }

    /// 使用特定 SSH 配置创建
    pub fn with_ssh_config(
        config: ConnectionConfig,
        ssh_config: SshConfig,
    ) -> Result<Self, TransportError> {
        let mut transport = Self::new(config)?;
        transport.ssh_config = ssh_config;
        Ok(transport)
    }

    /// 建立 SSH 隧道连接
    #[cfg(feature = "russh")]
    async fn connect_ssh_tunnel(&mut self) -> Result<(), TransportError> {
        use russh::client::Config;
        use russh::keys::key::KeyPair;
        use std::sync::Arc;

        // 创建 SSH 配置
        let config = Arc::new(Config::default());

        // 创建连接
        let addr = format!("{}:{}", self.ssh_config.ssh_host, self.ssh_config.ssh_port);
        let mut session = russh::client::connect(
            config,
            addr.parse().map_err(|e| TransportError::Connection(e.to_string()))?,
            Client {},
        )
        .await
        .map_err(|e| TransportError::Connection(e.to_string()))?;

        // 认证
        let auth_result = if let Some(ref _key_path) = self.ssh_config.private_key {
            // 使用密钥认证
            // let key_data = tokio::fs::read(key_path)
            //     .await
            //     .map_err(|e| TransportError::Io(e.to_string()))?;
            // 解析密钥并认证...
            // session.authenticate_publickey(...).await
            false
        } else if let Some(ref password) = self.ssh_config.password {
            // 使用密码认证
            session
                .authenticate_password(&self.ssh_config.username, password)
                .await
        } else {
            return Err(TransportError::Connection(
                "No authentication method provided".to_string(),
            ));
        };

        if !auth_result {
            return Err(TransportError::Connection("Authentication failed".to_string()));
        }

        // 建立端口转发
        let channel = session
            .channel_open_direct_tcpip(
                &self.ssh_config.remote_dest,
                self.config.addr.port(),
                "127.0.0.1",
                0,
            )
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        #[cfg(feature = "russh")]
        {
            self.ssh_session = Some(Arc::new(Mutex::new(session)));
        }

        // 设置接收任务
        // self.setup_receive_tasks(channel).await?;

        *self.connected.write().await = true;
        log::info!("SSH tunnel connected to {}", self.config.addr);

        Ok(())
    }

    /// 设置接收任务
    async fn setup_receive_tasks<C>(&mut self, _channel: C) -> Result<(), TransportError>
    where
        C: AsyncReadExt + AsyncWriteExt + Send + Unpin + 'static,
    {
        let (video_tx, video_rx) = mpsc::unbounded_channel();
        let (input_ack_tx, input_ack_rx) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

        self.video_rx = Some(video_rx);
        self.input_ack_rx = Some(input_ack_rx);
        self.control_rx = Some(control_rx);
        self.shutdown_tx = Some(shutdown_tx);

        let connected = self.connected.clone();

        // 接收任务
        tokio::spawn(async move {
            let mut _buf = vec![0u8; 65536];
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    // 接收逻辑...
                    else => {
                        // 读取数据并分发
                    }
                }
            }
        });

        // 统计任务
        let stats_connected = self.connected.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                if !*stats_connected.read().await {
                    break;
                }
            }
        });

        Ok(())
    }

    /// 简单的 TCP 直连（用于测试）
    async fn connect_tcp(&mut self, addr: SocketAddr) -> Result<(), TransportError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        let stream = Arc::new(Mutex::new(stream));
        self.stream = Some(stream.clone());

        // 启动接收任务
        self.start_tcp_receive_task(stream).await;

        *self.connected.write().await = true;
        log::info!("SSH tunnel (TCP fallback) connected to {}", addr);

        Ok(())
    }

    /// 启动 TCP 接收任务
    async fn start_tcp_receive_task(&mut self, stream: Arc<Mutex<TcpStream>>) {
        let (video_tx, video_rx) = mpsc::unbounded_channel::<VideoPacket>();
        let (input_ack_tx, input_ack_rx) = mpsc::unbounded_channel::<InputAck>();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<ControlPacket>();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

        self.video_rx = Some(video_rx);
        self.input_ack_rx = Some(input_ack_rx);
        self.control_rx = Some(control_rx);
        self.shutdown_tx = Some(shutdown_tx);

        let connected = self.connected.clone();
        let stats = self.stats.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    result = async {
                        let mut s = stream.lock().await;
                        s.read(&mut buf).await
                    } => {
                        match result {
                            Ok(0) => {
                                *connected.write().await = false;
                                break;
                            }
                            Ok(n) => {
                                // 解析并分发数据包
                                // 简化处理：假设所有数据都是视频包
                                stats.write().await.bytes_received += n as u64;
                                stats.write().await.packets_received += 1;
                            }
                            Err(e) => {
                                log::error!("SSH tunnel read error: {}", e);
                                *connected.write().await = false;
                                break;
                            }
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl Transport for SshTunnelTransport {
    async fn connect(&mut self, addr: SocketAddr) -> Result<(), TransportError> {
        // 尝试 SSH 隧道（如果配置了 russh）
        #[cfg(feature = "russh")]
        {
            return self.connect_ssh_tunnel().await;
        }

        // 回退到直连 TCP
        self.connect_tcp(addr).await
    }

    async fn send_video(&self, frame: &EncodedFrame) -> Result<(), TransportError> {
        let seq = self.video_seq.fetch_add(1, Ordering::SeqCst);
        let packet = VideoPacket {
            seq,
            timestamp: frame.pts,
            data: frame.data.clone(),
            key_frame: frame.key_frame,
            width: frame.width,
            height: frame.height,
            codec: frame.codec.clone(),
        };

        let mut buf = BytesMut::new();
        packet.encode(&mut buf);

        // 添加帧头
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(b"VIDEO"); // 魔数
        frame_data.extend_from_slice(&(buf.len() as u32).to_be_bytes()); // 长度
        frame_data.extend_from_slice(&buf);

        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&frame_data)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
            self.stats.write().await.bytes_sent += frame_data.len() as u64;
            self.stats.write().await.packets_sent += 1;
        } else {
            return Err(TransportError::NotConnected);
        }

        Ok(())
    }

    async fn send_input(&self, event: InputEvent) -> Result<(), TransportError> {
        let seq = self.input_seq.fetch_add(1, Ordering::SeqCst);
        let data = event.encode()?;

        // 构建输入帧
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(b"INPUT"); // 魔数
        frame_data.extend_from_slice(&seq.to_be_bytes()); // 序列号
        frame_data.extend_from_slice(&(data.len() as u32).to_be_bytes()); // 长度
        frame_data.extend_from_slice(&data);

        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&frame_data)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
        } else {
            return Err(TransportError::NotConnected);
        }

        Ok(())
    }

    async fn recv_video(&self) -> Result<VideoPacket, TransportError> {
        if let Some(ref mut rx) = self.video_rx.as_ref() {
            rx.recv()
                .await
                .ok_or(TransportError::Connection("Video channel closed".to_string()))
        } else {
            Err(TransportError::NotConnected)
        }
    }

    async fn recv_input_ack(&self) -> Result<InputAck, TransportError> {
        if let Some(ref mut rx) = self.input_ack_rx.as_ref() {
            rx.recv()
                .await
                .ok_or(TransportError::Connection("Input ack channel closed".to_string()))
        } else {
            Err(TransportError::NotConnected)
        }
    }

    async fn send_control(&self, packet: ControlPacket) -> Result<(), TransportError> {
        let data = packet.encode()?;

        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(b"CTRL");
        frame_data.extend_from_slice(&(data.len() as u32).to_be_bytes());
        frame_data.extend_from_slice(&data);

        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&frame_data)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
        }

        Ok(())
    }

    async fn recv_control(&self) -> Result<ControlPacket, TransportError> {
        if let Some(ref mut rx) = self.control_rx.as_ref() {
            rx.recv()
                .await
                .ok_or(TransportError::Connection("Control channel closed".to_string()))
        } else {
            Err(TransportError::NotConnected)
        }
    }

    fn stats(&self) -> NetworkStats {
        if let Ok(stats) = self.stats.try_read() {
            *stats
        } else {
            NetworkStats::default()
        }
    }

    fn is_connected(&self) -> bool {
        *self.connected.blocking_read()
    }

    async fn disconnect(&mut self) {
        // 发送关闭信号
        if let Some(ref tx) = self.shutdown_tx {
            let _ = tx.send(());
        }

        *self.connected.write().await = false;
        self.stream = None;

        #[cfg(feature = "russh")]
        {
            // self.ssh_session = None;
        }

        log::info!("SSH tunnel disconnected");
    }

    fn mode(&self) -> TransportMode {
        TransportMode::SshTunnel
    }
}

/// russh 客户端处理器
#[cfg(feature = "russh")]
struct Client;

#[cfg(feature = "russh")]
impl russh::client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // 开发环境接受所有密钥
        // 生产环境应该验证密钥指纹
        Ok(true)
    }
}

/// SSH 连接池
pub struct SshConnectionPool {
    connections: Vec<Arc<Mutex<SshTunnelTransport>>>,
    max_connections: usize,
}

impl SshConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            connections: Vec::new(),
            max_connections,
        }
    }

    /// 获取或创建连接
    pub async fn get_connection(
        &mut self,
        config: &ConnectionConfig,
        ssh_config: &SshConfig,
    ) -> Result<Arc<Mutex<SshTunnelTransport>>, TransportError> {
        // 查找现有连接
        for conn in &self.connections {
            if let Ok(transport) = conn.try_lock() {
                if transport.is_connected() {
                    return Ok(conn.clone());
                }
            }
        }

        // 创建新连接
        if self.connections.len() >= self.max_connections {
            return Err(TransportError::Connection(
                "Connection pool exhausted".to_string(),
            ));
        }

        let mut transport = SshTunnelTransport::with_ssh_config(config.clone(), ssh_config.clone())?;
        transport.connect(config.addr).await?;

        let conn = Arc::new(Mutex::new(transport));
        self.connections.push(conn.clone());

        Ok(conn)
    }

    /// 清理断开连接
    pub fn cleanup(&mut self) {
        self.connections
            .retain(|conn| conn.try_lock().map(|t| t.is_connected()).unwrap_or(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_tunnel_creation() {
        let config = ConnectionConfig::default();
        let transport = SshTunnelTransport::new(config);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert_eq!(config.ssh_port, 22);
        assert_eq!(config.username, "user");
    }

    #[tokio::test]
    async fn test_ssh_stats() {
        let config = ConnectionConfig::default();
        let transport = SshTunnelTransport::new(config).unwrap();
        let stats = transport.stats();
        assert_eq!(stats.bytes_sent, 0);
    }
}
