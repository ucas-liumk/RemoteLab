/**
 * 流协议定义
 * 
 * 定义 transport ↔ frontend 之间的通信协议
 * 
 * 协议设计目标:
 * 1. 最小开销 - 二进制数据高效传输
 * 2. 低延迟 - 减少序列化开销
 * 3. 可扩展 - 支持未来协议升级
 */

// ============================================================================
// 客户端 → 服务端 消息
// ============================================================================

export type ClientMessageType = 'input' | 'control' | 'ping' | 'quality';

/**
 * 客户端消息
 */
export interface ClientMessage {
    type: ClientMessageType;
    /** 发送时间戳 (微秒) - 用于 RTT 计算 */
    timestamp: number;
    payload: InputPayload | ControlPayload | PingPayload | QualityPayload;
}

/** 输入事件类型 */
export type InputEventType = 
    | 'mouse-move' 
    | 'mouse-down' 
    | 'mouse-up' 
    | 'mouse-wheel'
    | 'key-down' 
    | 'key-up';

/**
 * 输入事件载荷
 */
export interface InputPayload {
    /** 设备类型 */
    device?: 'mouse' | 'keyboard';
    /** 动作类型 */
    action?: 'move' | 'down' | 'up' | 'scroll';
    /** 鼠标 X 坐标 (相对于视频) */
    x?: number;
    /** 鼠标 Y 坐标 */
    y?: number;
    /** 鼠标按钮 (0=left, 1=middle, 2=right) */
    button?: number;
    /** 键盘按键代码 */
    keyCode?: string;
    /** 滚轮增量 */
    deltaY?: number;
    /** 修饰键状态 */
    modifiers?: {
        shift: boolean;
        ctrl: boolean;
        alt: boolean;
        meta: boolean;
    };
    /** 事件类型 (简化版) */
    eventType?: InputEventType;
    /** 输入预测序列号 - 用于服务器校正 */
    sequence: number;
}

/**
 * 控制命令载荷
 */
export interface ControlPayload {
    command: 'request-idr' | 'set-quality' | 'pause' | 'resume' | 'ping-response';
    /** 质量等级 (1-10) */
    value?: number;
}

/**
 * Ping 载荷
 */
export interface PingPayload {
    id: string;
}

/**
 * 质量报告载荷
 */
export interface QualityPayload {
    /** 当前延迟 (ms) */
    latency: number;
    /** 当前帧率 */
    frameRate: number;
    /** 丢包率 (0-1) */
    packetLoss: number;
    /** 建议码率 (bps) */
    suggestedBitrate?: number;
}

// ============================================================================
// 服务端 → 客户端 消息
// ============================================================================

export type ServerMessageType = 'video' | 'audio' | 'control' | 'pong' | 'cursor' | 'quality_ack';

/**
 * 服务端消息
 */
export interface ServerMessage {
    type: ServerMessageType;
    timestamp: number;
    payload: VideoPayload | AudioPayload | ControlResponse | PongPayload | CursorPayload;
}

/**
 * 视频帧载荷
 */
export interface VideoPayload {
    /** 帧 ID (用于统计和同步) */
    frameId: number;
    /** 编码格式 */
    codec: 'h264' | 'hevc' | 'av1';
    /** 帧类型 */
    frameType: 'idr' | 'i' | 'p';
    /** 视频数据 */
    data: ArrayBuffer;
    /** 视频宽度 */
    width: number;
    /** 视频高度 */
    height: number;
    /** 客户端解码时间戳 (服务器编码完成时间) */
    encodeTimestamp: number;
    /** 量化参数 (质量指标) */
    qp?: number;
}

/**
 * 音频帧载荷
 */
export interface AudioPayload {
    /** 时间戳 */
    timestamp: number;
    /** 音频数据 */
    data: ArrayBuffer;
    /** 采样率 */
    sampleRate: number;
    /** 声道数 */
    channels: number;
}

/**
 * 光标位置载荷 (用于预测校正)
 */
export interface CursorPayload {
    x: number;
    y: number;
    /** 对应客户端序列号 */
    sequence?: number;
    /** 服务器处理时间戳 */
    serverTimestamp?: number;
}

/**
 * 控制响应载荷
 */
export interface ControlResponse {
    command: 'bandwidth-update' | 'latency-report' | 'force-idr';
    /** 建议码率 (bps) */
    bandwidth?: number;
    /** 当前延迟 (ms) */
    latency?: number;
}

/**
 * Pong 响应载荷
 */
export interface PongPayload {
    id: string;
    /** 服务器当前时间 */
    serverTime: number;
}

// ============================================================================
// 网络统计
// ============================================================================

/**
 * 网络统计信息
 */
export interface NetworkStats {
    /** 往返延迟 (ms) */
    rttMs: number;
    /** 抖动 (ms) */
    jitterMs: number;
    /** 丢包率 (0.0-1.0) */
    packetLossRate: number;
    /** 可用带宽估计 (bps) */
    bandwidthBps: number;
    /** 拥塞窗口大小 */
    congestionWindow: number;
    /** 发送速率 (bps) */
    sendRateBps: number;
    /** 接收速率 (bps) */
    recvRateBps: number;
}

// ============================================================================
// 性能统计
// ============================================================================

/**
 * 性能统计信息
 */
export interface PerformanceStats {
    /** 捕获延迟 (ms) */
    captureLatency: number;
    /** 编码延迟 (ms) */
    encodeLatency: number;
    /** 传输延迟 (ms) */
    transmitLatency: number;
    /** 解码延迟 (ms) */
    decodeLatency: number;
    /** 渲染延迟 (ms) */
    renderLatency: number;
    /** 总延迟 (ms) */
    totalLatency: number;
    /** 当前帧率 */
    frameRate: number;
    /** 当前码率 (Mbps) */
    bitrateMbps: number;
    /** 是否达到目标延迟 (<16ms) */
    targetMet: boolean;
}

// ============================================================================
// 数据包编码/解码工具
// ============================================================================

/**
 * 消息编码器
 */
export class ProtocolEncoder {
    /**
     * 编码客户端消息
     */
    static encodeClientMessage(message: ClientMessage): ArrayBuffer {
        // 使用 JSON 编码 (可优化为二进制)
        const json = JSON.stringify(message);
        const encoder = new TextEncoder();
        return encoder.encode(json).buffer;
    }

    /**
     * 编码服务端消息
     */
    static encodeServerMessage(message: ServerMessage): ArrayBuffer {
        const json = JSON.stringify(message);
        const encoder = new TextEncoder();
        return encoder.encode(json).buffer;
    }

    /**
     * 创建输入消息
     */
    static createInputMessage(payload: InputPayload): ClientMessage {
        return {
            type: 'input',
            timestamp: performance.now(),
            payload,
        };
    }

    /**
     * 创建 Ping 消息
     */
    static createPingMessage(): ClientMessage {
        return {
            type: 'ping',
            timestamp: performance.now(),
            payload: { id: Math.random().toString(36).substr(2, 9) },
        };
    }

    /**
     * 创建控制消息
     */
    static createControlMessage(command: ControlPayload['command'], value?: number): ClientMessage {
        return {
            type: 'control',
            timestamp: performance.now(),
            payload: { command, value },
        };
    }

    /**
     * 创建质量报告消息
     */
    static createQualityReport(latency: number, frameRate: number, packetLoss: number): ClientMessage {
        return {
            type: 'quality',
            timestamp: performance.now(),
            payload: { latency, frameRate, packetLoss },
        };
    }
}

/**
 * 消息解码器
 */
export class ProtocolDecoder {
    /**
     * 解码服务端消息
     */
    static decodeServerMessage(data: ArrayBuffer): ServerMessage {
        const decoder = new TextDecoder();
        const json = decoder.decode(data);
        return JSON.parse(json);
    }

    /**
     * 解码客户端消息
     */
    static decodeClientMessage(data: ArrayBuffer): ClientMessage {
        const decoder = new TextDecoder();
        const json = decoder.decode(data);
        return JSON.parse(json);
    }
}

// ============================================================================
// 常量定义
// ============================================================================

/** 目标延迟 (ms) */
export const TARGET_LATENCY = 16;

/** 最大延迟 (ms) */
export const MAX_LATENCY = 50;

/** 帧率目标 */
export const TARGET_FRAME_RATE = 60;

/** 最大队列长度 */
export const MAX_QUEUE_LENGTH = 2;

/** 预测时间窗口 (ms) */
export const PREDICTION_HORIZON = 16;

/** 码率档位 (Mbps) */
export const BITRATE_LEVELS = {
    LOW: 5,
    MEDIUM: 10,
    HIGH: 20,
    ULTRA: 40,
} as const;

// ============================================================================
// 类型守卫
// ============================================================================

export function isVideoPayload(payload: unknown): payload is VideoPayload {
    return (
        typeof payload === 'object' &&
        payload !== null &&
        'frameId' in payload &&
        'codec' in payload &&
        'data' in payload
    );
}

export function isCursorPayload(payload: unknown): payload is CursorPayload {
    return (
        typeof payload === 'object' &&
        payload !== null &&
        'x' in payload &&
        'y' in payload
    );
}

export function isInputPayload(payload: unknown): payload is InputPayload {
    return (
        typeof payload === 'object' &&
        payload !== null &&
        'sequence' in payload
    );
}
