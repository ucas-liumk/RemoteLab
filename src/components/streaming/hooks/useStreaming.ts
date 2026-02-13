/**
 * Streaming React Hooks
 * 
 * 提供流媒体的 React hooks 接口
 * 简化 VideoPlayer 组件的使用
 */

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { WebCodecsDecoder, DecoderConfig } from '../WebCodecsDecoder';
import { InputPredictor, MousePrediction } from '../InputPredictor';
import { WebGLRenderer } from '../WebGLRenderer';
import { 
    VideoPayload, 
    PerformanceStats, 
    InputPayload,
    ProtocolEncoder,
    TARGET_LATENCY,
    TARGET_FRAME_RATE,
} from '../protocol';

// ============================================================================
// useStreaming Hook
// ============================================================================

export interface UseStreamingOptions {
    /** 流媒体 URL */
    url: string;
    /** 视频编码格式 */
    codec?: 'h264' | 'hevc' | 'av1';
    /** 是否启用输入预测 */
    enablePrediction?: boolean;
    /** 目标延迟 (ms) */
    targetLatency?: number;
}

export interface UseStreamingReturn {
    // 状态
    isConnected: boolean;
    isConnecting: boolean;
    error: Error | null;
    
    // 性能指标
    latency: number;
    fps: number;
    bitrate: number;
    
    // 统计
    stats: PerformanceStats | null;
    
    // 引用 (用于 VideoPlayer 组件)
    decoder: WebCodecsDecoder | null;
    renderer: WebGLRenderer | null;
    predictor: InputPredictor | null;
    
    // 方法
    connect: () => Promise<void>;
    disconnect: () => void;
    sendInput: (input: InputPayload) => void;
    resetStats: () => void;
}

/**
 * 流媒体核心 Hook
 * 
 * 管理 WebCodecs 解码器、WebGL 渲染器和输入预测器的生命周期
 */
export function useStreaming(options: UseStreamingOptions): UseStreamingReturn {
    const { url, codec = 'h264', enablePrediction = true, targetLatency = TARGET_LATENCY } = options;
    
    // 状态
    const [isConnected, setIsConnected] = useState(false);
    const [isConnecting, setIsConnecting] = useState(false);
    const [error, setError] = useState<Error | null>(null);
    const [latency, setLatency] = useState(0);
    const [fps, setFps] = useState(0);
    const [bitrate, setBitrate] = useState(0);
    const [stats, setStats] = useState<PerformanceStats | null>(null);
    
    // 引用
    const decoderRef = useRef<WebCodecsDecoder | null>(null);
    const rendererRef = useRef<WebGLRenderer | null>(null);
    const predictorRef = useRef<InputPredictor | null>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const receiveLoopRef = useRef<boolean>(false);
    
    // 内部统计
    const statsRef = useRef({
        framesReceived: 0,
        bytesReceived: 0,
        lastFpsTime: performance.now(),
        frameCount: 0,
        latencySum: 0,
        latencyCount: 0,
    });

    /**
     * 连接到服务器
     */
    const connect = useCallback(async () => {
        if (isConnected || isConnecting) return;
        
        setIsConnecting(true);
        setError(null);
        
        try {
            // 初始化 WebSocket
            wsRef.current = new WebSocket(url);
            wsRef.current.binaryType = 'arraybuffer';
            
            await new Promise<void>((resolve, reject) => {
                if (!wsRef.current) {
                    reject(new Error('WebSocket not initialized'));
                    return;
                }
                
                wsRef.current.onopen = () => {
                    setIsConnected(true);
                    setIsConnecting(false);
                    resolve();
                };
                
                wsRef.current.onerror = (e) => {
                    reject(new Error('WebSocket connection failed'));
                };
                
                wsRef.current.onclose = () => {
                    setIsConnected(false);
                    receiveLoopRef.current = false;
                };
                
                wsRef.current.onmessage = (event) => {
                    handleMessage(event.data);
                };
            });
            
            // 启动接收循环
            receiveLoopRef.current = true;
            
        } catch (err) {
            setError(err instanceof Error ? err : new Error('Unknown error'));
            setIsConnecting(false);
            throw err;
        }
    }, [url, isConnected, isConnecting]);

    /**
     * 断开连接
     */
    const disconnect = useCallback(() => {
        receiveLoopRef.current = false;
        wsRef.current?.close();
        wsRef.current = null;
        setIsConnected(false);
    }, []);

    /**
     * 处理消息
     */
    const handleMessage = useCallback((data: ArrayBuffer | string) => {
        try {
            let message: any;
            
            if (typeof data === 'string') {
                message = JSON.parse(data);
            } else {
                const decoder = new TextDecoder();
                message = JSON.parse(decoder.decode(data));
            }
            
            switch (message.type) {
                case 'video':
                    handleVideoMessage(message.payload as VideoPayload);
                    break;
                case 'pong':
                    handlePongMessage(message.payload);
                    break;
                case 'cursor':
                    handleCursorMessage(message.payload);
                    break;
            }
        } catch (err) {
            console.error('Message handling error:', err);
        }
    }, []);

    /**
     * 处理视频消息
     */
    const handleVideoMessage = useCallback((payload: VideoPayload) => {
        const receiveTime = performance.now();
        
        statsRef.current.framesReceived++;
        statsRef.current.bytesReceived += payload.data.byteLength;
        
        // 解码
        if (decoderRef.current) {
            decoderRef.current.decode(payload);
        }
        
        // 计算延迟
        const frameLatency = receiveTime - payload.encodeTimestamp;
        statsRef.current.latencySum += frameLatency;
        statsRef.current.latencyCount++;
        
        // 更新 FPS
        const now = performance.now();
        if (now - statsRef.current.lastFpsTime >= 1000) {
            const elapsed = (now - statsRef.current.lastFpsTime) / 1000;
            setFps(Math.round(statsRef.current.frameCount / elapsed));
            
            // 计算码率
            const bits = statsRef.current.bytesReceived * 8;
            setBitrate(Math.round(bits / elapsed / 1000000 * 10) / 10);
            
            // 更新平均延迟
            if (statsRef.current.latencyCount > 0) {
                const avgLatency = statsRef.current.latencySum / statsRef.current.latencyCount;
                setLatency(Math.round(avgLatency * 10) / 10);
            }
            
            // 更新统计
            setStats({
                captureLatency: 0,
                encodeLatency: 0,
                transmitLatency: frameLatency,
                decodeLatency: 0,
                renderLatency: 0,
                totalLatency: frameLatency,
                frameRate: statsRef.current.frameCount,
                bitrateMbps: bitrate,
                targetMet: frameLatency < targetLatency,
            });
            
            // 重置计数器
            statsRef.current.frameCount = 0;
            statsRef.current.bytesReceived = 0;
            statsRef.current.latencySum = 0;
            statsRef.current.latencyCount = 0;
            statsRef.current.lastFpsTime = now;
        }
        
        statsRef.current.frameCount++;
    }, [bitrate, targetLatency]);

    /**
     * 处理 Pong 消息
     */
    const handlePongMessage = useCallback((payload: any) => {
        const rtt = performance.now() - payload.clientTime;
        console.log('RTT:', rtt.toFixed(2), 'ms');
    }, []);

    /**
     * 处理光标校正消息
     */
    const handleCursorMessage = useCallback((payload: any) => {
        predictorRef.current?.onServerCorrection({ x: payload.x, y: payload.y });
    }, []);

    /**
     * 发送输入
     */
    const sendInput = useCallback((input: InputPayload) => {
        if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return;
        
        const message = ProtocolEncoder.createInputMessage(input);
        wsRef.current.send(JSON.stringify(message));
    }, []);

    /**
     * 发送 Ping
     */
    const sendPing = useCallback(() => {
        if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return;
        
        const message = ProtocolEncoder.createPingMessage();
        wsRef.current.send(JSON.stringify(message));
    }, []);

    /**
     * 重置统计
     */
    const resetStats = useCallback(() => {
        statsRef.current = {
            framesReceived: 0,
            bytesReceived: 0,
            lastFpsTime: performance.now(),
            frameCount: 0,
            latencySum: 0,
            latencyCount: 0,
        };
        decoderRef.current?.resetStats();
        setLatency(0);
        setFps(0);
        setBitrate(0);
    }, []);

    // 定期发送 Ping
    useEffect(() => {
        if (!isConnected) return;
        
        const interval = setInterval(sendPing, 1000);
        return () => clearInterval(interval);
    }, [isConnected, sendPing]);

    // 清理
    useEffect(() => {
        return () => {
            disconnect();
            decoderRef.current?.close();
            rendererRef.current?.destroy();
        };
    }, [disconnect]);

    return {
        isConnected,
        isConnecting,
        error,
        latency,
        fps,
        bitrate,
        stats,
        decoder: decoderRef.current,
        renderer: rendererRef.current,
        predictor: predictorRef.current,
        connect,
        disconnect,
        sendInput,
        resetStats,
    };
}

// ============================================================================
// useVideoDecoder Hook
// ============================================================================

export interface UseVideoDecoderOptions {
    config: DecoderConfig;
    canvas?: HTMLCanvasElement | null;
    onFrame?: (frame: any) => void;
    onError?: (error: Error) => void;
}

/**
 * 视频解码器 Hook
 */
export function useVideoDecoder(options: UseVideoDecoderOptions) {
    const { config, canvas, onFrame, onError } = options;
    const decoderRef = useRef<WebCodecsDecoder | null>(null);
    const [isReady, setIsReady] = useState(false);

    useEffect(() => {
        const init = async () => {
            try {
                decoderRef.current = new WebCodecsDecoder(config);
                await decoderRef.current.init(canvas ?? undefined);
                
                if (onFrame) {
                    decoderRef.current.onFrame(onFrame);
                }
                if (onError) {
                    decoderRef.current.onError(onError);
                }
                
                setIsReady(true);
            } catch (err) {
                onError?.(err instanceof Error ? err : new Error('Decoder init failed'));
            }
        };

        init();

        return () => {
            decoderRef.current?.close();
            decoderRef.current = null;
            setIsReady(false);
        };
    }, [config.codec, config.width, config.height, canvas]);

    const decode = useCallback((payload: VideoPayload) => {
        decoderRef.current?.decode(payload);
    }, []);

    const flush = useCallback(async () => {
        await decoderRef.current?.flush();
    }, []);

    const getStats = useCallback(() => {
        return decoderRef.current?.getStats();
    }, []);

    return {
        isReady,
        decode,
        flush,
        getStats,
        decoder: decoderRef.current,
    };
}

// ============================================================================
// useInputPrediction Hook
// ============================================================================

export interface UseInputPredictionOptions {
    enabled?: boolean;
    onCorrection?: (error: { x: number; y: number }) => void;
}

export interface UseInputPredictionReturn {
    predict: (x: number, y: number) => MousePrediction | null;
    correct: (serverPos: { x: number; y: number }) => void;
    getAccuracy: () => number;
    stats: ReturnType<InputPredictor['getStats']> | null;
    setEnabled: (enabled: boolean) => void;
}

/**
 * 输入预测 Hook
 */
export function useInputPrediction(options: UseInputPredictionOptions = {}): UseInputPredictionReturn {
    const { enabled = true, onCorrection } = options;
    const predictorRef = useRef<InputPredictor | null>(null);
    const [stats, setStats] = useState<ReturnType<InputPredictor['getStats']> | null>(null);

    useEffect(() => {
        predictorRef.current = new InputPredictor();
        predictorRef.current.setEnabled(enabled);
        
        if (onCorrection) {
            predictorRef.current.onCorrection(onCorrection);
        }

        return () => {
            predictorRef.current?.reset();
            predictorRef.current = null;
        };
    }, [enabled, onCorrection]);

    const predict = useCallback((x: number, y: number): MousePrediction | null => {
        if (!predictorRef.current) return null;
        return predictorRef.current.onMouseMove(x, y);
    }, []);

    const correct = useCallback((serverPos: { x: number; y: number }) => {
        predictorRef.current?.onServerCorrection(serverPos);
        setStats(predictorRef.current?.getStats() ?? null);
    }, []);

    const getAccuracy = useCallback(() => {
        return predictorRef.current?.getAccuracy() ?? 0;
    }, []);

    const setEnabled = useCallback((enabled: boolean) => {
        predictorRef.current?.setEnabled(enabled);
    }, []);

    return {
        predict,
        correct,
        getAccuracy,
        stats,
        setEnabled,
    };
}

// ============================================================================
// useWebGLRenderer Hook
// ============================================================================

export interface UseWebGLRendererOptions {
    canvas: HTMLCanvasElement | null;
    targetFrameRate?: number;
}

/**
 * WebGL 渲染器 Hook
 */
export function useWebGLRenderer(options: UseWebGLRendererOptions) {
    const { canvas, targetFrameRate = TARGET_FRAME_RATE } = options;
    const rendererRef = useRef<WebGLRenderer | null>(null);
    const [isReady, setIsReady] = useState(false);

    useEffect(() => {
        if (!canvas) return;

        try {
            rendererRef.current = new WebGLRenderer({
                canvas,
                targetFrameRate,
            });
            setIsReady(true);
        } catch (err) {
            console.error('WebGL init failed:', err);
        }

        return () => {
            rendererRef.current?.destroy();
            rendererRef.current = null;
            setIsReady(false);
        };
    }, [canvas, targetFrameRate]);

    const render = useCallback((frame: any) => {
        if (rendererRef.current && frame instanceof VideoFrame) {
            rendererRef.current.render(frame);
        }
    }, []);

    const clear = useCallback(() => {
        rendererRef.current?.clear();
    }, []);

    const resize = useCallback((width: number, height: number) => {
        rendererRef.current?.resize(width, height);
    }, []);

    const getStats = useCallback(() => {
        return rendererRef.current?.getStats();
    }, []);

    return {
        isReady,
        render,
        clear,
        resize,
        getStats,
        renderer: rendererRef.current,
    };
}

// ============================================================================
// usePerformanceMonitor Hook
// ============================================================================

export interface UsePerformanceMonitorOptions {
    onReport?: (stats: PerformanceStats) => void;
    reportInterval?: number;
}

/**
 * 性能监控 Hook
 */
export function usePerformanceMonitor(options: UsePerformanceMonitorOptions = {}) {
    const { onReport, reportInterval = 1000 } = options;
    const [stats, setStats] = useState<PerformanceStats | null>(null);
    const statsRef = useRef({
        frames: 0,
        latencySum: 0,
        startTime: performance.now(),
    });

    const recordFrame = useCallback((latency: number) => {
        statsRef.current.frames++;
        statsRef.current.latencySum += latency;
    }, []);

    const report = useCallback(() => {
        const elapsed = (performance.now() - statsRef.current.startTime) / 1000;
        const avgLatency = statsRef.current.latencySum / Math.max(statsRef.current.frames, 1);
        const fps = Math.round(statsRef.current.frames / elapsed);

        const newStats: PerformanceStats = {
            captureLatency: 0,
            encodeLatency: 0,
            transmitLatency: avgLatency,
            decodeLatency: 0,
            renderLatency: 0,
            totalLatency: avgLatency,
            frameRate: fps,
            bitrateMbps: 0,
            targetMet: avgLatency < TARGET_LATENCY,
        };

        setStats(newStats);
        onReport?.(newStats);

        // 重置
        statsRef.current = {
            frames: 0,
            latencySum: 0,
            startTime: performance.now(),
        };
    }, [onReport]);

    useEffect(() => {
        const interval = setInterval(report, reportInterval);
        return () => clearInterval(interval);
    }, [report, reportInterval]);

    return {
        stats,
        recordFrame,
        report,
    };
}

export default useStreaming;
