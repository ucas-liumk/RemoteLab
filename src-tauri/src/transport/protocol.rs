//! RemoteLab Transport Protocol Definitions
//! 
//! 定义了视频、输入事件和控制数据包的二进制编码格式
//! 支持低延迟传输和可靠传输两种模式

use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

/// 传输错误类型
#[derive(Debug, Clone)]
pub enum TransportError {
    Io(String),
    Codec(String),
    Connection(String),
    Timeout,
    NotConnected,
    InvalidPacket,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "IO error: {}", e),
            TransportError::Codec(e) => write!(f, "Codec error: {}", e),
            TransportError::Connection(e) => write!(f, "Connection error: {}", e),
            TransportError::Timeout => write!(f, "Operation timeout"),
            TransportError::NotConnected => write!(f, "Not connected"),
            TransportError::InvalidPacket => write!(f, "Invalid packet"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        TransportError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(e: serde_json::Error) -> Self {
        TransportError::Codec(e.to_string())
    }
}

/// 网络统计信息
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NetworkStats {
    /// 往返时延（毫秒）
    pub rtt_ms: f32,
    /// 丢包率（0-1）
    pub loss_rate: f32,
    /// 带宽估计（bps）
    pub bandwidth_bps: u32,
    /// 抖动（毫秒）
    pub jitter_ms: f32,
    /// 发送字节数
    pub bytes_sent: u64,
    /// 接收字节数
    pub bytes_received: u64,
    /// 发送包数
    pub packets_sent: u64,
    /// 接收包数
    pub packets_received: u64,
    /// 丢包数
    pub packets_lost: u64,
}

/// 视频数据包
/// 
/// 用于传输编码后的视频帧数据
/// 支持 H.264/HEVC 编码格式
#[derive(Debug, Clone)]
pub struct VideoPacket {
    /// 序列号（用于丢包检测和排序）
    pub seq: u32,
    /// 时间戳（微秒）
    pub timestamp: u64,
    /// 编码数据
    pub data: Vec<u8>,
    /// 是否关键帧
    pub key_frame: bool,
    /// 视频宽度
    pub width: u32,
    /// 视频高度
    pub height: u32,
    /// 编码格式
    pub codec: VideoCodec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    VP9,
    AV1,
}

impl VideoPacket {
    /// 包头大小（不含 data）
    pub const HEADER_SIZE: usize = 4 + 8 + 4 + 1 + 4 + 4 + 1;

    /// 编码为字节
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.reserve(self.data.len() + Self::HEADER_SIZE);
        buf.put_u32(self.seq);
        buf.put_u64(self.timestamp);
        buf.put_u32(self.data.len() as u32);
        buf.put_u8(if self.key_frame { 1 } else { 0 });
        buf.put_u32(self.width);
        buf.put_u32(self.height);
        buf.put_u8(self.codec as u8);
        buf.put_slice(&self.data);
    }

    /// 从字节解码
    pub fn decode(buf: &[u8]) -> Result<Self, TransportError> {
        if buf.len() < Self::HEADER_SIZE - 1 {
            return Err(TransportError::InvalidPacket);
        }

        let mut cursor = buf;
        let seq = cursor.get_u32();
        let timestamp = cursor.get_u64();
        let data_len = cursor.get_u32() as usize;
        let key_frame = cursor.get_u8() != 0;
        let width = cursor.get_u32();
        let height = cursor.get_u32();
        let codec = match cursor.get_u8() {
            0 => VideoCodec::H264,
            1 => VideoCodec::H265,
            2 => VideoCodec::VP9,
            3 => VideoCodec::AV1,
            _ => VideoCodec::H264,
        };

        if cursor.remaining() < data_len {
            return Err(TransportError::InvalidPacket);
        }

        let data = cursor[..data_len].to_vec();

        Ok(Self {
            seq,
            timestamp,
            data,
            key_frame,
            width,
            height,
            codec,
        })
    }

    /// 获取当前时间戳（微秒）
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }
}

impl Default for VideoPacket {
    fn default() -> Self {
        Self {
            seq: 0,
            timestamp: 0,
            data: Vec::new(),
            key_frame: false,
            width: 1920,
            height: 1080,
            codec: VideoCodec::H264,
        }
    }
}

/// 输入事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    /// 鼠标移动
    MouseMove { x: f32, y: f32 },
    /// 鼠标按下
    MouseDown { button: u8, x: f32, y: f32 },
    /// 鼠标释放
    MouseUp { button: u8, x: f32, y: f32 },
    /// 鼠标滚轮
    MouseWheel { delta_x: f32, delta_y: f32 },
    /// 键盘按下
    KeyDown { keycode: u32, modifiers: u8 },
    /// 键盘释放
    KeyUp { keycode: u32, modifiers: u8 },
    /// 字符输入
    CharInput { character: String },
}

impl InputEvent {
    /// 编码为 JSON 字节
    pub fn encode(&self) -> Result<Vec<u8>, TransportError> {
        serde_json::to_vec(self).map_err(Into::into)
    }

    /// 从 JSON 字节解码
    pub fn decode(buf: &[u8]) -> Result<Self, TransportError> {
        serde_json::from_slice(buf).map_err(Into::into)
    }
}

/// 输入确认包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputAck {
    /// 确认的事件序列号
    pub seq: u64,
    /// 确认时间戳
    pub timestamp: u64,
    /// 处理结果
    pub success: bool,
}

impl InputAck {
    pub fn encode(&self) -> Result<Vec<u8>, TransportError> {
        serde_json::to_vec(self).map_err(Into::into)
    }

    pub fn decode(buf: &[u8]) -> Result<Self, TransportError> {
        serde_json::from_slice(buf).map_err(Into::into)
    }
}

/// 控制包类型（用于连接管理）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlPacket {
    /// 连接请求
    Connect { client_version: String, capabilities: Vec<String> },
    /// 连接响应
    ConnectResponse { success: bool, server_version: String, session_id: String },
    /// 心跳 Ping
    Ping { timestamp: u64 },
    /// 心跳 Pong
    Pong { timestamp: u64 },
    /// 断开连接
    Disconnect { reason: String },
    /// 带宽探测
    BandwidthProbe { seq: u32, size: u32 },
    /// 带宽探测响应
    BandwidthAck { seq: u32, received_at: u64 },
    /// 视频参数更新
    VideoParams { bitrate: u32, fps: u8, resolution: (u32, u32) },
}

impl ControlPacket {
    pub fn encode(&self) -> Result<Vec<u8>, TransportError> {
        serde_json::to_vec(self).map_err(Into::into)
    }

    pub fn decode(buf: &[u8]) -> Result<Self, TransportError> {
        serde_json::from_slice(buf).map_err(Into::into)
    }
}

/// 编码后的视频帧（来自 encoder 模块）
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    /// 显示时间戳（微秒）
    pub pts: u64,
    /// 编码数据
    pub data: Vec<u8>,
    /// 是否关键帧
    pub key_frame: bool,
    /// 视频宽度
    pub width: u32,
    /// 视频高度
    pub height: u32,
    /// 编码格式
    pub codec: VideoCodec,
}

/// 传输模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// QUIC 协议
    Quic,
    /// WebTransport 协议
    WebTransport,
    /// SSH 隧道（回退）
    SshTunnel,
}

/// 连接配置
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// 目标地址
    pub addr: std::net::SocketAddr,
    /// 传输模式
    pub mode: TransportMode,
    /// TLS 证书路径（可选）
    pub cert_path: Option<String>,
    /// 连接超时（秒）
    pub timeout_secs: u64,
    /// 视频码率（bps）
    pub video_bitrate: u32,
    /// 帧率
    pub fps: u8,
    /// 分辨率
    pub resolution: (u32, u32),
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8080".parse().unwrap(),
            mode: TransportMode::Quic,
            cert_path: None,
            timeout_secs: 10,
            video_bitrate: 10_000_000, // 10 Mbps
            fps: 60,
            resolution: (1920, 1080),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_packet_encode_decode() {
        let packet = VideoPacket {
            seq: 42,
            timestamp: 12345678,
            data: vec![1, 2, 3, 4, 5],
            key_frame: true,
            width: 1920,
            height: 1080,
            codec: VideoCodec::H264,
        };

        let mut buf = BytesMut::new();
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

    #[test]
    fn test_input_event_json() {
        let event = InputEvent::MouseMove { x: 100.5, y: 200.0 };
        let encoded = event.encode().unwrap();
        let decoded = InputEvent::decode(&encoded).unwrap();
        
        match decoded {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 100.5);
                assert_eq!(y, 200.0);
            }
            _ => panic!("Wrong event type"),
        }
    }
}
