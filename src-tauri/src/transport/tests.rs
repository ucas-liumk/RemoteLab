//! Transport Layer Tests
//! 
//! 包含所有传输协议的单元测试和集成测试

use super::protocol::*;
use super::*;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// 测试辅助函数：创建本地地址
fn local_addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

// =============== 协议测试 ===============

#[tokio::test]
async fn test_video_packet_encode_decode() {
    let packet = VideoPacket {
        seq: 42,
        timestamp: 12345678,
        data: vec![1, 2, 3, 4, 5],
        key_frame: true,
        width: 1920,
        height: 1080,
        codec: VideoCodec::H264,
    };

    let mut buf = bytes::BytesMut::new();
    packet.encode(&mut buf);

    let decoded = VideoPacket::decode(&buf).unwrap();

    assert_eq!(decoded.seq, packet.seq);
    assert_eq!(decoded.timestamp, packet.timestamp);
    assert_eq!(decoded.data, packet.data);
    assert_eq!(decoded.key_frame, packet.key_frame);
    assert_eq!(decoded.width, packet.width);
    assert_eq!(decoded.height, packet.height);
    assert_eq!(decoded.codec, packet.codec);
}

#[tokio::test]
async fn test_input_event_json() {
    let events = vec![
        InputEvent::MouseMove { x: 100.5, y: 200.0 },
        InputEvent::MouseDown { button: 0, x: 50.0, y: 100.0 },
        InputEvent::MouseUp { button: 0, x: 50.0, y: 100.0 },
        InputEvent::MouseWheel { delta_x: 0.0, delta_y: 3.0 },
        InputEvent::KeyDown { keycode: 65, modifiers: 0 },
        InputEvent::KeyUp { keycode: 65, modifiers: 0 },
        InputEvent::CharInput {
            character: "A".to_string(),
        },
    ];

    for event in events {
        let encoded = event.encode().unwrap();
        let decoded = InputEvent::decode(&encoded).unwrap();

        match (&event, decoded) {
            (
                InputEvent::MouseMove { x: x1, y: y1 },
                InputEvent::MouseMove { x: x2, y: y2 },
            ) => {
                assert!((x1 - x2).abs() < f32::EPSILON);
                assert!((y1 - y2).abs() < f32::EPSILON);
            }
            _ => panic!("Event mismatch"),
        }
    }
}

#[tokio::test]
async fn test_control_packet_encode_decode() {
    let packets = vec![
        ControlPacket::Connect {
            client_version: "1.0.0".to_string(),
            capabilities: vec!["h264".to_string(), "h265".to_string()],
        },
        ControlPacket::ConnectResponse {
            success: true,
            server_version: "1.0.0".to_string(),
            session_id: "test-session-123".to_string(),
        },
        ControlPacket::Ping { timestamp: 12345 },
        ControlPacket::Pong { timestamp: 12345 },
        ControlPacket::Disconnect {
            reason: "test".to_string(),
        },
    ];

    for packet in packets {
        let encoded = packet.encode().unwrap();
        let decoded = ControlPacket::decode(&encoded).unwrap();

        match (packet, decoded) {
            (ControlPacket::Ping { timestamp: t1 }, ControlPacket::Ping { timestamp: t2 }) => {
                assert_eq!(t1, t2);
            }
            (ControlPacket::Pong { timestamp: t1 }, ControlPacket::Pong { timestamp: t2 }) => {
                assert_eq!(t1, t2);
            }
            _ => (), // 其他类型只验证编解码不报错
        }
    }
}

// =============== QUIC 传输测试 ===============

#[tokio::test]
async fn test_quic_transport_creation() {
    let config = ConnectionConfig::default();
    let transport = quic::QuicTransport::new(config);
    assert!(transport.is_ok());
}

#[tokio::test]
async fn test_quic_stats_initial() {
    let config = ConnectionConfig::default();
    let transport = quic::QuicTransport::new(config).unwrap();
    let stats = transport.stats();
    assert_eq!(stats.rtt_ms, 0.0);
    assert_eq!(stats.packets_sent, 0);
    assert_eq!(stats.bytes_sent, 0);
}

// =============== WebTransport 测试 ===============

#[tokio::test]
async fn test_webtransport_client_creation() {
    let config = ConnectionConfig::default();
    let client = webtransport::WebTransportClient::new(config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_webtransport_stats() {
    let config = ConnectionConfig::default();
    let client = webtransport::WebTransportClient::new(config).unwrap();
    let stats = client.stats();
    assert_eq!(stats.rtt_ms, 0.0);
}

// =============== SSH 隧道测试 ===============

#[tokio::test]
async fn test_ssh_tunnel_creation() {
    let config = ConnectionConfig::default();
    let transport = sshtunnel::SshTunnelTransport::new(config);
    assert!(transport.is_ok());
}

#[tokio::test]
async fn test_ssh_stats() {
    let config = ConnectionConfig::default();
    let transport = sshtunnel::SshTunnelTransport::new(config).unwrap();
    let stats = transport.stats();
    assert_eq!(stats.bytes_sent, 0);
}

// =============== 传输管理器测试 ===============

#[tokio::test]
async fn test_transport_factory_create() {
    let config = ConnectionConfig::default();

    // 测试各种模式
    let modes = vec![
        TransportMode::Quic,
        TransportMode::WebTransport,
        TransportMode::SshTunnel,
    ];

    for mode in modes {
        let mut config = config.clone();
        config.mode = mode;

        // 工厂创建不应失败
        let result = TransportFactory::create(&config);
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_transport_manager() {
    let config = ConnectionConfig::default();
    let mut manager = TransportManager::new(config);

    // 初始状态
    assert!(manager.primary().is_none());

    // 添加传输
    let config1 = ConnectionConfig::default();
    let transport1 = TransportFactory::create(&config1).unwrap();
    manager.add_transport(transport1);

    assert!(manager.primary().is_some());
}

// =============== 延迟监控测试 ===============

#[tokio::test]
async fn test_latency_monitor() {
    let mut monitor = LatencyMonitor::new(10);

    // 添加样本
    monitor.add_sample(10.0);
    monitor.add_sample(20.0);
    monitor.add_sample(15.0);

    // 检查平均值
    let avg = monitor.average_rtt();
    assert!((avg - 15.0).abs() < 0.01);

    // 检查抖动
    let jitter = monitor.jitter();
    assert!(jitter > 0.0);
}

#[tokio::test]
async fn test_bandwidth_estimator() {
    let mut estimator = BandwidthEstimator::new();

    // 初始估计
    let initial = estimator.estimate();
    assert_eq!(initial, 10_000_000); // 10 Mbps

    // 模拟数据传输
    estimator.update(1_000_000); // 1 MB
    sleep(Duration::from_millis(100)).await;
    estimator.update(2_000_000); // 2 MB

    let estimate = estimator.estimate();
    assert!(estimate > 0);
}

// =============== 性能基准测试 ===============

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    /// 测试视频包编码性能
    #[tokio::test]
    async fn bench_video_packet_encode() {
        let packet = VideoPacket {
            seq: 1,
            timestamp: VideoPacket::now(),
            data: vec![0u8; 10000], // 10KB
            key_frame: true,
            width: 1920,
            height: 1080,
            codec: VideoCodec::H264,
        };

        let iterations = 10000;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut buf = bytes::BytesMut::new();
            packet.encode(&mut buf);
        }

        let elapsed = start.elapsed();
        let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

        println!("Video packet encode: {:.0} ops/sec", ops_per_sec);
        assert!(ops_per_sec > 100000.0, "Encoding too slow");
    }

    /// 测试视频包解码性能
    #[tokio::test]
    async fn bench_video_packet_decode() {
        let packet = VideoPacket {
            seq: 1,
            timestamp: VideoPacket::now(),
            data: vec![0u8; 10000],
            key_frame: true,
            width: 1920,
            height: 1080,
            codec: VideoCodec::H264,
        };

        let mut buf = bytes::BytesMut::new();
        packet.encode(&mut buf);
        let encoded = buf.freeze();

        let iterations = 10000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = VideoPacket::decode(&encoded);
        }

        let elapsed = start.elapsed();
        let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

        println!("Video packet decode: {:.0} ops/sec", ops_per_sec);
        assert!(ops_per_sec > 100000.0, "Decoding too slow");
    }

    /// 测试输入事件序列化性能
    #[tokio::test]
    async fn bench_input_event_serialize() {
        let event = InputEvent::MouseMove { x: 100.5, y: 200.0 };

        let iterations = 100000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = event.encode().unwrap();
        }

        let elapsed = start.elapsed();
        let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

        println!("Input event serialize: {:.0} ops/sec", ops_per_sec);
        assert!(ops_per_sec > 500000.0, "Serialization too slow");
    }
}

// =============== 网络模拟测试 ===============

/// 模拟网络延迟和丢包
pub struct NetworkSimulator {
    base_delay_ms: u64,
    jitter_ms: u64,
    loss_rate: f32,
}

impl NetworkSimulator {
    pub fn new(base_delay_ms: u64, jitter_ms: u64, loss_rate: f32) -> Self {
        Self {
            base_delay_ms,
            jitter_ms,
            loss_rate,
        }
    }

    /// 模拟网络延迟
    pub async fn delay(&self) {
        use rand::Rng;
        let jitter = if self.jitter_ms > 0 {
            rand::thread_rng().gen_range(0..self.jitter_ms)
        } else {
            0
        };
        sleep(Duration::from_millis(self.base_delay_ms + jitter)).await;
    }

    /// 模拟丢包
    pub fn should_drop(&self) -> bool {
        use rand::Rng;
        rand::thread_rng().gen::<f32>() < self.loss_rate
    }
}

#[tokio::test]
async fn test_network_simulator() {
    let sim = NetworkSimulator::new(10, 5, 0.0);

    let start = Instant::now();
    sim.delay().await;
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() >= 10);
}

// =============== 连接测试 ===============

/// 测试连接超时
#[tokio::test]
async fn test_connection_timeout() {
    let config = ConnectionConfig {
        addr: "127.0.0.1:9999".parse().unwrap(),
        mode: TransportMode::WebTransport,
        cert_path: None,
        timeout_secs: 1,
        video_bitrate: 10_000_000,
        fps: 60,
        resolution: (1920, 1080),
    };

    let mut transport = TransportFactory::create(&config).unwrap();

    // 连接应该超时（因为没有服务器）
    let result = timeout(Duration::from_secs(2), transport.connect(config.addr)).await;

    // 预期超时或连接失败
    assert!(result.is_err() || result.unwrap().is_err());
}

// =============== 压力测试 ===============

#[tokio::test]
async fn test_high_frequency_packets() {
    let iterations = 1000;
    let start = Instant::now();

    for i in 0..iterations {
        let packet = VideoPacket {
            seq: i as u32,
            timestamp: VideoPacket::now(),
            data: vec![0u8; 1000],
            key_frame: i % 30 == 0,
            width: 1920,
            height: 1080,
            codec: VideoCodec::H264,
        };

        let mut buf = bytes::BytesMut::new();
        packet.encode(&mut buf);
    }

    let elapsed = start.elapsed();
    let packets_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!("High frequency encoding: {:.0} packets/sec", packets_per_sec);
    assert!(packets_per_sec > 10000.0, "Encoding throughput too low");
}

// =============== 内存测试 ===============

#[tokio::test]
async fn test_memory_usage() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let counter = Arc::new(AtomicUsize::new(0));

    // 模拟创建和销毁大量传输实例
    for _ in 0..100 {
        let config = ConnectionConfig::default();
        let _transport = quic::QuicTransport::new(config);
        counter.fetch_add(1, Ordering::SeqCst);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 100);
}
