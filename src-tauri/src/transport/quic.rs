//! QUIC Transport Implementation
//! 
//! 使用 quinn 库实现高性能 QUIC 传输
//! 特点：
//! - 视频数据使用 unreliable datagram（低延迟）
//! - 输入事件使用可靠 bidirectional stream
//! - 内置拥塞控制和连接迁移

use super::protocol::*;
use super::Transport;
use async_trait::async_trait;
use bytes::BytesMut;
use quinn::{ClientConfig, Endpoint, RecvStream, SendDatagramError, SendStream};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, Duration, Instant};

/// QUIC 传输实现
pub struct QuicTransport {
    /// QUIC 端点
    endpoint: Option<Endpoint>,
    /// 客户端配置
    client_config: Option<ClientConfig>,
    /// 活动连接
    connection: Option<quinn::Connection>,
    /// 视频发送流（用于可靠传输备选）
    video_stream: Option<Arc<Mutex<SendStream>>>,
    /// 输入发送流
    input_stream: Option<Arc<Mutex<SendStream>>>,
    /// 控制发送流
    control_stream: Option<Arc<Mutex<SendStream>>>,
    /// 配置
    config: ConnectionConfig,
    /// 连接状态
    connected: Arc<RwLock<bool>>,
    /// 网络统计
    stats: Arc<RwLock<NetworkStats>>,
    /// 视频序列号
    video_seq: AtomicU32,
    /// 输入序列号
    input_seq: AtomicU64,
    /// 统计收集任务句柄
    stats_task: Option<tokio::task::AbortHandle>,
    /// 视频接收通道
    video_rx: Option<mpsc::UnboundedReceiver<VideoPacket>>,
    /// 输入确认接收通道
    input_ack_rx: Option<mpsc::UnboundedReceiver<InputAck>>,
    /// 控制包接收通道
    control_rx: Option<mpsc::UnboundedReceiver<ControlPacket>>,
    /// 关闭信号
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

/// 自定义证书验证器（开发测试用）
#[derive(Debug)]
pub struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // 开发环境跳过证书验证
        // 生产环境应使用正确配置的证书
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

impl QuicTransport {
    /// 创建新的 QUIC 传输实例
    pub fn new(config: ConnectionConfig) -> Result<Self, TransportError> {
        // 创建 TLS 配置
        let tls_config = create_client_tls_config(config.cert_path.as_deref())?;
        let client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
                .map_err(|e| TransportError::Connection(e.to_string()))?
        ));

        // 创建端点
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let endpoint = Endpoint::client(bind_addr)
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        Ok(Self {
            client_config: Some(client_config),
            endpoint: Some(endpoint),
            connection: None,
            video_stream: None,
            input_stream: None,
            control_stream: None,
            config,
            connected: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            video_seq: AtomicU32::new(0),
            input_seq: AtomicU64::new(0),
            stats_task: None,
            video_rx: None,
            input_ack_rx: None,
            control_rx: None,
            shutdown_tx: None,
            client_config: None,
        })
    }

    /// 启动接收任务
    fn start_receive_tasks(
        &mut self,
        video_recv: RecvStream,
        input_recv: RecvStream,
        control_recv: RecvStream,
        conn: quinn::Connection,
    ) {
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

        // 视频接收任务
        tokio::spawn(async move {
            let mut recv = video_recv;
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    result = recv.read_chunk(65536, false) => {
                        match result {
                            Ok(Some(chunk)) => {
                                if let Ok(packet) = VideoPacket::decode(&chunk.bytes) {
                                    let _ = video_tx.send(packet);
                                    stats.write().await.bytes_received += chunk.bytes.len() as u64;
                                    stats.write().await.packets_received += 1;
                                }
                            }
                            Ok(None) => continue,
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        // 输入确认接收任务
        let mut shutdown_rx2 = self.shutdown_tx.as_ref().unwrap().subscribe();
        tokio::spawn(async move {
            let mut recv = input_recv;
            let mut buf = vec![0u8; 1024];
            loop {
                tokio::select! {
                    _ = shutdown_rx2.recv() => break,
                    result = recv.read(&mut buf) => {
                        match result {
                            Ok(Some(n)) => {
                                if let Ok(ack) = InputAck::decode(&buf[..n]) {
                                    let _ = input_ack_tx.send(ack);
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        // 控制包接收任务
        let mut shutdown_rx3 = self.shutdown_tx.as_ref().unwrap().subscribe();
        tokio::spawn(async move {
            let mut recv = control_recv;
            let mut buf = vec![0u8; 4096];
            loop {
                tokio::select! {
                    _ = shutdown_rx3.recv() => break,
                    result = recv.read(&mut buf) => {
                        match result {
                            Ok(Some(n)) => {
                                if let Ok(packet) = ControlPacket::decode(&buf[..n]) {
                                    // 处理 Pong 包
                                    if let ControlPacket::Pong { timestamp } = packet {
                                        let rtt = VideoPacket::now() - timestamp;
                                        let rtt_ms = (rtt as f32) / 1000.0;
                                        // RTT 更新会在 stats 收集任务中处理
                                    }
                                    let _ = control_tx.send(packet);
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        // 统计收集任务
        let stats_clone = self.stats.clone();
        let connected_clone = self.connected.clone();
        let stats_handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            let mut last_ping = Instant::now();

            loop {
                interval.tick().await;

                if !*connected_clone.read().await {
                    break;
                }

                // 每秒发送一次 ping
                if last_ping.elapsed().as_secs() >= 1 {
                    let ping = ControlPacket::Ping {
                        timestamp: VideoPacket::now(),
                    };
                    if let Ok(data) = ping.encode() {
                        // 通过 datagram 发送 ping
                        let _ = conn.send_datagram(data.into());
                    }
                    last_ping = Instant::now();
                }
            }
        });

        self.stats_task = Some(stats_handle.abort_handle());
    }

    /// 收集统计信息
    async fn collect_stats(&self) {
        // 统计信息通过接收任务自动更新
    }
}

#[async_trait]
impl Transport for QuicTransport {
    async fn connect(&mut self, addr: SocketAddr) -> Result<(), TransportError> {
        if self.endpoint.is_none() || self.client_config.is_none() {
            return Err(TransportError::NotConnected);
        }

        let endpoint = self.endpoint.as_ref().unwrap();
        let client_config = self.client_config.as_ref().unwrap();
        let server_name = ServerName::try_from("remotelab")
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        // 建立连接
        let conn = endpoint
            .connect_with(
                client_config.clone(),
                addr,
                server_name,
            )
            .map_err(|e| TransportError::Connection(e.to_string()))?
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        // 创建双向流
        let (video_send, video_recv) = conn
            .open_bi()
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        let (input_send, input_recv) = conn
            .open_bi()
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        let (control_send, control_recv) = conn
            .open_bi()
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        // 发送连接请求
        let connect_req = ControlPacket::Connect {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec!["h264".to_string(), "h265".to_string()],
        };
        let req_data = connect_req.encode()?;
        let mut control = control_send;
        control
            .write_all(&req_data)
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?;

        self.video_stream = Some(Arc::new(Mutex::new(video_send)));
        self.input_stream = Some(Arc::new(Mutex::new(input_send)));
        self.control_stream = Some(Arc::new(Mutex::new(control)));

        *self.connected.write().await = true;
        self.connection = Some(conn.clone());

        // 启动接收任务
        self.start_receive_tasks(video_recv, input_recv, control_recv, conn);

        log::info!("QUIC connected to {}", addr);
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

        // 优先使用 datagram（不可靠但低延迟）
        if let Some(ref conn) = self.connection {
            match conn.send_datagram(buf.freeze().into()) {
                Ok(()) => {
                    self.stats.write().await.bytes_sent += buf.len() as u64;
                    self.stats.write().await.packets_sent += 1;
                    return Ok(());
                }
                Err(SendDatagramError::UnsupportedByPeer) => {
                    // 降级到 stream 传输
                }
                Err(e) => return Err(TransportError::Connection(e.to_string())),
            }
        }

        // 回退到 stream 传输
        if let Some(ref stream) = self.video_stream {
            let mut s = stream.lock().await;
            s.write_all(&buf)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
            self.stats.write().await.bytes_sent += buf.len() as u64;
        }

        Ok(())
    }

    async fn send_input(&self, event: InputEvent) -> Result<(), TransportError> {
        let seq = self.input_seq.fetch_add(1, Ordering::SeqCst);
        let data = event.encode()?;

        if let Some(ref stream) = self.input_stream {
            let mut s = stream.lock().await;
            // 先发序列号（8字节）
            s.write_all(&seq.to_be_bytes())
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
            // 再发数据长度（4字节）
            s.write_all(&(data.len() as u32).to_be_bytes())
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
            // 发数据
            s.write_all(&data)
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

        // 控制包使用 stream 保证可靠性
        if let Some(ref stream) = self.control_stream {
            let mut s = stream.lock().await;
            s.write_all(&data)
                .await
                .map_err(|e| TransportError::Io(e.to_string()))?;
        } else if let Some(ref conn) = self.connection {
            // 尝试使用 datagram
            let _ = conn.send_datagram(data.into());
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
        // 使用 try_lock 避免阻塞
        if let Ok(stats) = self.stats.try_read() {
            *stats
        } else {
            NetworkStats::default()
        }
    }

    fn is_connected(&self) -> bool {
        self.connection.is_some()
            && self
                .connection
                .as_ref()
                .map(|c| c.close_reason().is_none())
                .unwrap_or(false)
    }

    async fn disconnect(&mut self) {
        // 发送关闭信号
        if let Some(ref tx) = self.shutdown_tx {
            let _ = tx.send(());
        }

        // 取消统计任务
        if let Some(handle) = self.stats_task.take() {
            handle.abort();
        }

        // 关闭流
        self.video_stream = None;
        self.input_stream = None;
        self.control_stream = None;

        // 关闭连接
        if let Some(conn) = self.connection.take() {
            conn.close(0u32.into(), b"client disconnect");
        }

        // 关闭端点
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close(0u32.into(), b"client shutdown");
        }

        *self.connected.write().await = false;
        log::info!("QUIC disconnected");
    }

    fn mode(&self) -> TransportMode {
        TransportMode::Quic
    }
}

/// 创建 TLS 客户端配置
fn create_client_tls_config(
    cert_path: Option<&str>,
) -> Result<rustls::ClientConfig, TransportError> {
    let mut roots = rustls::RootCertStore::empty();

    if let Some(path) = cert_path {
        // 加载自定义证书
        let cert = std::fs::read(path)
            .map_err(|e| TransportError::Io(format!("Failed to read cert: {}", e)))?;
        let cert = rustls_pemfile::certs(&mut cert.as_slice())
            .next()
            .ok_or_else(|| TransportError::Io("No certificate found".to_string()))?
            .map_err(|e| TransportError::Io(format!("Invalid cert: {}", e)))?;
        roots.add(cert).map_err(|e| TransportError::Io(format!("Failed to add cert: {}", e)))?;
    }

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quic_transport_creation() {
        let config = ConnectionConfig::default();
        let transport = QuicTransport::new(config);
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_stats_initial() {
        let config = ConnectionConfig::default();
        let transport = QuicTransport::new(config).unwrap();
        let stats = transport.stats();
        assert_eq!(stats.rtt_ms, 0.0);
        assert_eq!(stats.packets_sent, 0);
    }
}
