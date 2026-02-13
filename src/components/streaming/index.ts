/**
 * 超低延迟流媒体模块
 * 
 * RemoteLab Ultra 核心流媒体组件
 * 
 * 性能目标:
 * - 解码延迟 < 5ms
 * - 渲染延迟 < 3ms
 * - 输入响应 < 16ms (1帧)
 * - 支持 4K@60fps
 */

// 核心组件
export { VideoPlayer } from './VideoPlayer';
export { WebCodecsDecoder, createDecoder } from './WebCodecsDecoder';
export { WebGLRenderer, createRenderer } from './WebGLRenderer';
export { InputPredictor, createPredictor } from './InputPredictor';

// React Hooks
export {
    useStreaming,
    useVideoDecoder,
    useInputPrediction,
    useWebGLRenderer,
    usePerformanceMonitor,
} from './hooks/useStreaming';

// 协议定义
export {
    ProtocolEncoder,
    ProtocolDecoder,
    TARGET_LATENCY,
    TARGET_FRAME_RATE,
    MAX_QUEUE_LENGTH,
    PREDICTION_HORIZON,
    BITRATE_LEVELS,
    isVideoPayload,
    isCursorPayload,
    isInputPayload,
} from './protocol';

// 类型定义
export type {
    DecodedFrame,
    DecoderConfig,
    CodecType,
} from './WebCodecsDecoder';

export type {
    RenderOptions,
} from './WebGLRenderer';

export type {
    InputEvent,
    MousePrediction,
    KeyPrediction,
    PredictedInput,
    CorrectionData,
} from './InputPredictor';

export type {
    UseStreamingOptions,
    UseStreamingReturn,
} from './hooks/useStreaming';

export type {
    ClientMessage,
    ServerMessage,
    VideoPayload,
    AudioPayload,
    ControlPayload,
    ControlResponse,
    InputPayload,
    CursorPayload,
    PingPayload,
    PongPayload,
    QualityPayload,
    NetworkStats,
    PerformanceStats,
    ClientMessageType,
    ServerMessageType,
    InputEventType,
} from './protocol';
