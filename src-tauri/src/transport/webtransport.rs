//! WebTransport Client Implementation
//! 
//! WebTransport 是基于 HTTP/3 的现代 Web 传输协议
//! 特点：
//! - 基于 QUIC，支持可靠和不可靠传输
//! - 浏览器原生支持
//! - 适合 Web 客户端

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
use tokio::time::{interval, Duration, Instant};

/// WebTransport 客户端实现
/// 
/// 注意：由于 wtransport crate 需要特定环境，这里使用条件编译
/// 在实际环境中启用 webtransport 功能
pub struct WebTransportClient {
    /// 连接状态
    connected: Arc<RwLock<bool>>,
    /// 连接配置
    config: ConnectionConfig,
    /// 网络统计
    stats: Arc<RwLock<NetworkStats>>,
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
    /// 内部 TCP 流（回退实现）
    stream: Option<Arc<Mutex<TcpStream>>>,
}

impl WebTransportClient {
    /// 创建新的 WebTransport 客户端
    pub fn new(config: ConnectionConfig) -> Result<Self, TransportError> {
        Ok(Self {
            connected: Arc::new(RwLock::new(false)),
            config,
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            video_seq: AtomicU32::new(0),
            input_seq: AtomicU64::new(0),
            video_rx: None,
            input_ack_rx: None,
            control_rx: None,
            shutdown_tx: None,
            stream: None,
        })
    }

    /// 创建 wtransport 连接（需要启用 wtransport feature）
    #[cfg(feature = "wtransport")]
    pub async fn connect_wt(&mut self, url: &str) -> Result<(), TransportError> {
        use wtransport::{ClientConfig, WebTransport};

        let config = ClientConfig::builder()
            .with_bind_address("0.0.0.0:0".parse().unwrap())
            .with_no_cert_validation(); // 开发环境

        let connection = WebTransport::connect(url, &config)
            .await
            .map_err(|e| TransportError::Connection(format!("WebTransport connect failed: {}", e)))?;

        // 创建双向流
        let stream = connection
            .open_bi_stream()
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        *self.connected.write().await = true;

        // 设置接收任务
        self.setup_receive_tasks().await?;

        log::info!("WebTransport connected to {}", url);
        Ok(())
    }

    /// 设置接收任务
    async fn setup_receive_tasks(&mut self) -> Result<(), TransportError> {
        let (video_tx, video_rx) = mpsc::unbounded_channel();
        let (input_ack_tx, input_ack_rx) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

        self.video_rx = Some(video_rx);
        self.input_ack_rx = Some(input_ack_rx);
        self.control_rx = Some(control_rx);
        self.shutdown_tx = Some(shutdown_tx);

        let connected = self.connected.clone();
        let stats = self.stats.clone();

        // 启动统计收集任务
        let stats_handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                if !*connected.read().await {
                    break;
                }
                // 更新统计
            }
        });

        Ok(())
    }
}

#[async_trait]
impl Transport for WebTransportClient {
    async fn connect(&mut self, addr: SocketAddr) -> Result<(), TransportError> {
        // WebTransport 通常使用 URL，这里构建 URL
        let url = format!("https://{}:{}/remotelab", addr.ip(), addr.port());

        // 尝试 WebTransport 连接
        #[cfg(feature = "wtransport")]
        {
            return self.connect_wt(&url).await;
        }

        // 回退到 TCP 实现（开发测试用）
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        self.stream = Some(Arc::new(Mutex::new(stream)));
        *self.connected.write().await = true;

        // 设置接收任务
        self.setup_receive_tasks().await?;

        log::info!("WebTransport (TCP fallback) connected to {}", addr);
        Ok(())
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

        // 使用不可靠传输（datagram）如果可用
        #[cfg(feature = "wtransport")]
        {
            // WebTransport datagram API
        }

        // 回退到流式传输
        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&buf)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
            self.stats.write().await.bytes_sent += buf.len() as u64;
            self.stats.write().await.packets_sent += 1;
        }

        Ok(())
    }

    async fn send_input(&self, event: InputEvent) -> Result<(), TransportError> {
        let seq = self.input_seq.fetch_add(1, Ordering::SeqCst);
        let data = event.encode()?;

        // 构建可靠传输数据包
        let mut packet = Vec::new();
        packet.extend_from_slice(&seq.to_be_bytes()); // 8 bytes seq
        packet.extend_from_slice(&(data.len() as u32).to_be_bytes()); // 4 bytes len
        packet.extend_from_slice(&data);

        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&packet)
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

        if let Some(ref stream) = self.stream {
            let mut s = stream.lock().await;
            s.write_all(&data)
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

        log::info!("WebTransport disconnected");
    }

    fn mode(&self) -> TransportMode {
        TransportMode::WebTransport
    }
}

/// WebTransport 服务端（用于测试）
pub struct WebTransportServer {
    addr: SocketAddr,
}

impl WebTransportServer {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    /// 启动服务端
    #[cfg(feature = "wtransport")]
    pub async fn start(&self) -> Result<(), TransportError> {
        use wtransport::tls::CertificateChain;
        use wtransport::ServerConfig;

        // 生成自签名证书
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
            .map_err(|e| TransportError::Io(e.to_string()))?;
        let cert_der = cert.cert.der().clone();
        let key_der = cert.key_pair.serialize_der().into();

        let config = ServerConfig::builder()
            .with_bind_address(self.addr)
            .with_certificate(CertificateChain::single(cert_der.into()))
            .build();

        let server = wtransport::Server::builder()
            .with_config(config)
            .build()
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        log::info!("WebTransport server listening on {}", self.addr);

        // 接受连接
        loop {
            match server.accept().await {
                Ok(connection) => {
                    tokio::spawn(handle_connection(connection));
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }

    /// 不使用 wtransport 的测试服务端
    #[cfg(not(feature = "wtransport"))]
    pub async fn start(&self) -> Result<(), TransportError> {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(self.addr)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        log::info!("WebTransport (TCP) server listening on {}", self.addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::info!("New connection from {}", addr);
                    tokio::spawn(handle_tcp_connection(stream));
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }
}

#[cfg(feature = "wtransport")]
async fn handle_connection(connection: wtransport::Connection) -> Result<(), TransportError> {
    log::info!("New WebTransport connection");

    loop {
        match connection.accept_bi_stream().await {
            Ok(stream) => {
                tokio::spawn(handle_bi_stream(stream));
            }
            Err(e) => {
                log::error!("Stream accept error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

#[cfg(feature = "wtransport")]
async fn handle_bi_stream(
    (mut send, mut recv): (wtransport::SendStream, wtransport::RecvStream),
) -> Result<(), TransportError> {
    let mut buf = vec![0u8; 65536];

    loop {
        match recv.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                // 回声测试
                send.write_all(&buf[..n])
                    .await
                    .map_err(|e| TransportError::Io(e.to_string()))?;
            }
            Err(e) => {
                log::error!("Read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

async fn handle_tcp_connection(mut stream: TcpStream) -> Result<(), TransportError> {
    let mut buf = vec![0u8; 65536];

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                stream
                    .write_all(&buf[..n])
                    .await
                    .map_err(|e| TransportError::Io(e.to_string()))?;
            }
            Err(e) => {
                log::error!("Read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webtransport_client_creation() {
        let config = ConnectionConfig::default();
        let client = WebTransportClient::new(config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_webtransport_stats() {
        let config = ConnectionConfig::default();
        let client = WebTransportClient::new(config).unwrap();
        let stats = client.stats();
        assert_eq!(stats.rtt_ms, 0.0);
    }
}
