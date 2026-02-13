/**
 * 超低延迟视频播放器组件
 * 
 * 整合 WebCodecs + WebGL + InputPredictor 实现 <16ms 延迟播放
 * 
 * 性能目标:
 * - 解码延迟 < 5ms
 * - 渲染延迟 < 3ms
 * - 输入响应 < 16ms (1帧)
 * - 支持 4K@60fps
 */

import React, { useEffect, useRef, useCallback, useState } from 'react';
import { WebCodecsDecoder, DecodedFrame, DecoderConfig } from './WebCodecsDecoder';
import { WebGLRenderer } from './WebGLRenderer';
import { InputPredictor, MousePrediction, InputEvent } from './InputPredictor';
import { 
    VideoPayload, 
    NetworkStats, 
    PerformanceStats, 
    ClientMessage, 
    ServerMessage,
    InputPayload,
    CursorPayload,
} from './protocol';

// WebTransport 客户端类型声明
interface WebTransportClient {
    connect(url: string): Promise<void>;
    close(): void;
    receiveVideo(): Promise<VideoPayload>;
    sendInput(input: InputPayload): void;
    isConnected(): boolean;
}

interface VideoPlayerProps {
    /** WebTransport/WebSocket URL */
    url: string;
    /** 视频宽度 */
    width?: number;
    /** 视频高度 */
    height?: number;
    /** 视频编码格式 */
    codec?: 'h264' | 'hevc' | 'av1';
    /** 是否启用输入预测 */
    enablePrediction?: boolean;
    /** 性能报告回调 */
    onPerformanceReport?: (stats: PerformanceStats) => void;
    /** 延迟更新回调 */
    onLatencyUpdate?: (latency: number) => void;
    /** 连接状态回调 */
    onConnectionChange?: (connected: boolean) => void;
    /** 自定义类名 */
    className?: string;
}

/**
 * 视频播放器组件
 * 
 * 功能:
 * 1. WebCodecs 硬件解码
 * 2. WebGL 零拷贝渲染
 * 3. 输入预测降低感知延迟
 * 4. 自适应光标显示
 */
export const VideoPlayer: React.FC<VideoPlayerProps> = ({
    url,
    width = 1920,
    height = 1080,
    codec = 'h264' as const,
    enablePrediction = true,
    onPerformanceReport,
    onLatencyUpdate,
    onConnectionChange,
    className = '',
}) => {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const decoderRef = useRef<WebCodecsDecoder | null>(null);
    const rendererRef = useRef<WebGLRenderer | null>(null);
    const predictorRef = useRef<InputPredictor | null>(null);
    const transportRef = useRef<WebTransportClient | null>(null);
    const receiveLoopRef = useRef<boolean>(false);
    
    // 本地光标位置 (预测位置)
    const [cursorPos, setCursorPos] = useState({ x: 0, y: 0 });
    const [showLocalCursor, setShowLocalCursor] = useState(true);
    
    // 连接状态
    const [isConnected, setIsConnected] = useState(false);
    const [isConnecting, setIsConnecting] = useState(false);
    
    // 性能统计
    const [latency, setLatency] = useState(0);
    const [fps, setFps] = useState(0);
    const [bitrate, setBitrate] = useState(0);
    
    // 内部统计
    const statsRef = useRef({
        framesReceived: 0,
        framesDecoded: 0,
        bytesReceived: 0,
        lastReportTime: 0,
        lastFpsTime: 0,
        frameCount: 0,
    });

    // 初始化
    useEffect(() => {
        if (!canvasRef.current) return;

        const init = async () => {
            setIsConnecting(true);
            
            try {
                // 1. 初始化渲染器
                rendererRef.current = new WebGLRenderer({
                    canvas: canvasRef.current,
                    useYUV: false,
                    targetFrameRate: 60,
                });

                // 2. 初始化解码器
                const decoderConfig: DecoderConfig = {
                    codec,
                    width,
                    height,
                    hardwareAcceleration: 'prefer-hardware',
                    optimizeForLatency: true,
                };

                decoderRef.current = new WebCodecsDecoder(decoderConfig);
                await decoderRef.current.init(canvasRef.current);

                decoderRef.current.onFrame((frame: DecodedFrame) => {
                    statsRef.current.framesDecoded++;
                    statsRef.current.frameCount++;
                });

                decoderRef.current.onError((error: Error) => {
                    console.error('Decoder error:', error);
                    // 可以在这里请求 IDR 帧恢复
                });

                // 3. 初始化输入预测器
                predictorRef.current = new InputPredictor();
                predictorRef.current.setEnabled(enablePrediction);
                
                // 注册校正回调
                predictorRef.current.onCorrection((error) => {
                    // 平滑修正本地光标位置
                    setCursorPos(prev => ({
                        x: prev.x - error.x,
                        y: prev.y - error.y,
                    }));
                });

                // 4. 连接传输层
                await initTransport();

                setIsConnected(true);
                onConnectionChange?.(true);
                
            } catch (error) {
                console.error('Initialization error:', error);
                setIsConnected(false);
                onConnectionChange?.(false);
            } finally {
                setIsConnecting(false);
            }
        };

        init();

        // 清理
        return () => {
            cleanup();
        };
    }, [url, codec, width, height, enablePrediction]);

    /**
     * 初始化传输层
     */
    const initTransport = async () => {
        // 这里使用 WebSocket 作为示例，实际应使用 WebTransport
        // 创建模拟的 WebTransport 客户端
        transportRef.current = createWebSocketTransport(url, {
            onVideo: handleVideoMessage,
            onCursor: handleCursorCorrection,
            onConnect: () => {
                setIsConnected(true);
                onConnectionChange?.(true);
                startReceiveLoop();
            },
            onDisconnect: () => {
                setIsConnected(false);
                onConnectionChange?.(false);
                receiveLoopRef.current = false;
            },
        });
        
        await transportRef.current.connect(url);
    };

    /**
     * 开始接收循环
     */
    const startReceiveLoop = async () => {
        if (receiveLoopRef.current) return;
        receiveLoopRef.current = true;
        
        while (receiveLoopRef.current && transportRef.current?.isConnected()) {
            try {
                const packet = await transportRef.current.receiveVideo();
                handleVideoMessage(packet);
            } catch (error) {
                console.error('Receive error:', error);
                break;
            }
        }
    };

    /**
     * 处理视频消息
     */
    const handleVideoMessage = useCallback((payload: VideoPayload) => {
        const receiveTime = performance.now();
        statsRef.current.framesReceived++;
        statsRef.current.bytesReceived += payload.data.byteLength;

        // 计算传输延迟
        const transmitLatency = receiveTime - payload.encodeTimestamp;

        // 解码
        const decodeStartTime = performance.now();
        decoderRef.current?.decode(payload);
        const decodeLatency = performance.now() - decodeStartTime;

        // 总延迟
        const totalLatency = transmitLatency + decodeLatency;
        setLatency(totalLatency);
        onLatencyUpdate?.(totalLatency);

        // 更新 FPS
        const now = performance.now();
        if (now - statsRef.current.lastFpsTime >= 1000) {
            const elapsed = (now - statsRef.current.lastFpsTime) / 1000;
            setFps(Math.round(statsRef.current.frameCount / elapsed));
            
            // 计算码率 (Mbps)
            const bits = statsRef.current.bytesReceived * 8;
            const mbps = bits / elapsed / 1000000;
            setBitrate(Math.round(mbps * 10) / 10);
            
            statsRef.current.frameCount = 0;
            statsRef.current.bytesReceived = 0;
            statsRef.current.lastFpsTime = now;
        }

        // 性能报告
        if (now - statsRef.current.lastReportTime > 1000) {
            reportPerformance(transmitLatency, decodeLatency);
        }
    }, [onLatencyUpdate]);

    /**
     * 处理光标校正
     */
    const handleCursorCorrection = useCallback((payload: CursorPayload) => {
        // 应用服务器校正
        predictorRef.current?.onServerCorrection({ x: payload.x, y: payload.y });
    }, []);

    /**
     * 报告性能统计
     */
    const reportPerformance = useCallback((transmitLatency: number, decodeLatency: number) => {
        const rendererStats = rendererRef.current?.getStats();
        const decoderStats = decoderRef.current?.getStats();

        const stats: PerformanceStats = {
            captureLatency: 0, // 未知
            encodeLatency: 0,  // 未知
            transmitLatency,
            decodeLatency,
            renderLatency: rendererStats?.averageRenderTime ?? 0,
            totalLatency: transmitLatency + decodeLatency + (rendererStats?.averageRenderTime ?? 0),
            frameRate: statsRef.current.framesReceived,
            bitrateMbps: bitrate,
            targetMet: transmitLatency + decodeLatency < 16,
        };

        onPerformanceReport?.(stats);
        statsRef.current.lastReportTime = performance.now();
    }, [bitrate, onPerformanceReport]);

    /**
     * 处理鼠标移动 (带预测)
     */
    const handleMouseMove = useCallback((e: React.MouseEvent) => {
        if (!canvasRef.current || !predictorRef.current) return;

        const rect = canvasRef.current.getBoundingClientRect();
        const scaleX = width / rect.width;
        const scaleY = height / rect.height;
        const x = (e.clientX - rect.left) * scaleX;
        const y = (e.clientY - rect.top) * scaleY;

        // 本地预测
        const prediction = predictorRef.current.onMouseMove(x, y);
        
        // 立即更新本地光标位置 (预测位置)
        setCursorPos(prediction.predicted);
        setShowLocalCursor(true);

        // 发送实际位置给服务器
        const inputPayload: InputPayload = {
            eventType: 'mouse-move',
            x: prediction.actual.x,
            y: prediction.actual.y,
            sequence: 0,
        };

        transportRef.current?.sendInput(inputPayload);
    }, [width, height]);

    /**
     * 处理鼠标按下
     */
    const handleMouseDown = useCallback((e: React.MouseEvent) => {
        if (!canvasRef.current || !predictorRef.current) return;

        const rect = canvasRef.current.getBoundingClientRect();
        const scaleX = width / rect.width;
        const scaleY = height / rect.height;
        const x = (e.clientX - rect.left) * scaleX;
        const y = (e.clientY - rect.top) * scaleY;

        const event = predictorRef.current.onMouseDown(x, y, e.button);
        
        const inputPayload: InputPayload = {
            eventType: 'mouse-down',
            x,
            y,
            button: e.button,
            sequence: event.sequence,
        };

        transportRef.current?.sendInput(inputPayload);
        
        // 聚焦画布以接收键盘事件
        canvasRef.current.focus();
    }, [width, height]);

    /**
     * 处理鼠标释放
     */
    const handleMouseUp = useCallback((e: React.MouseEvent) => {
        if (!canvasRef.current || !predictorRef.current) return;

        const rect = canvasRef.current.getBoundingClientRect();
        const scaleX = width / rect.width;
        const scaleY = height / rect.height;
        const x = (e.clientX - rect.left) * scaleX;
        const y = (e.clientY - rect.top) * scaleY;

        const event = predictorRef.current.onMouseUp(x, y, e.button);
        
        const inputPayload: InputPayload = {
            eventType: 'mouse-up',
            x,
            y,
            button: e.button,
            sequence: event.sequence,
        };

        transportRef.current?.sendInput(inputPayload);
    }, [width, height]);

    /**
     * 处理键盘按下
     */
    const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
        if (!predictorRef.current) return;

        const prediction = predictorRef.current.onKeyDown(e.key);
        
        const inputPayload: InputPayload = {
            eventType: 'key-down',
            keyCode: e.key,
            modifiers: {
                shift: e.shiftKey,
                ctrl: e.ctrlKey,
                alt: e.altKey,
                meta: e.metaKey,
            },
            sequence: 0,
        };

        transportRef.current?.sendInput(inputPayload);
        e.preventDefault();
    }, []);

    /**
     * 处理键盘释放
     */
    const handleKeyUp = useCallback((e: React.KeyboardEvent) => {
        if (!predictorRef.current) return;

        const inputPayload: InputPayload = {
            eventType: 'key-up',
            keyCode: e.key,
            modifiers: {
                shift: e.shiftKey,
                ctrl: e.ctrlKey,
                alt: e.altKey,
                meta: e.metaKey,
            },
            sequence: 0,
        };

        transportRef.current?.sendInput(inputPayload);
        e.preventDefault();
    }, []);

    /**
     * 清理资源
     */
    const cleanup = () => {
        receiveLoopRef.current = false;
        decoderRef.current?.close();
        rendererRef.current?.destroy();
        transportRef.current?.close();
        predictorRef.current?.reset();
    };

    return (
        <div className={`relative ${className}`}>
            {/* 视频画布 */}
            <canvas
                ref={canvasRef}
                width={width}
                height={height}
                className="cursor-none outline-none"
                onMouseMove={handleMouseMove}
                onMouseDown={handleMouseDown}
                onMouseUp={handleMouseUp}
                onKeyDown={handleKeyDown}
                onKeyUp={handleKeyUp}
                tabIndex={0}
            />
            
            {/* 本地预测光标 */}
            {showLocalCursor && (
                <div
                    className="pointer-events-none fixed w-4 h-4 -ml-2 -mt-2 z-50"
                    style={{
                        left: cursorPos.x,
                        top: cursorPos.y,
                        transform: 'translate(0, 0)', // 使用 GPU 加速
                    }}
                >
                    <svg 
                        width="16" 
                        height="16" 
                        viewBox="0 0 16 16" 
                        fill="none"
                        className="drop-shadow-md"
                    >
                        <path 
                            d="M0 0L14 8L8 10L6 16L0 0Z" 
                            fill="white" 
                            stroke="black" 
                            strokeWidth="1"
                        />
                    </svg>
                </div>
            )}
            
            {/* 状态显示 */}
            <div className="absolute top-2 left-2 bg-black/70 text-white px-3 py-2 rounded text-sm font-mono backdrop-blur-sm">
                {isConnecting ? (
                    <span className="text-yellow-400">Connecting...</span>
                ) : isConnected ? (
                    <div className="flex flex-col gap-1">
                        <div className="flex items-center gap-2">
                            <span className={latency < 16 ? 'text-green-400' : 'text-yellow-400'}>
                                {latency.toFixed(1)}ms
                            </span>
                            <span className="text-gray-400">|</span>
                            <span className="text-blue-400">{fps} FPS</span>
                            <span className="text-gray-400">|</span>
                            <span className="text-purple-400">{bitrate.toFixed(1)} Mbps</span>
                        </div>
                        <div className="text-xs text-gray-400">
                            Prediction: {enablePrediction ? 'ON' : 'OFF'}
                        </div>
                    </div>
                ) : (
                    <span className="text-red-400">Disconnected</span>
                )}
            </div>
        </div>
    );
};

/**
 * 创建 WebSocket 传输客户端
 * 实际实现中应使用 WebTransport
 */
function createWebSocketTransport(
    url: string,
    handlers: {
        onVideo: (payload: VideoPayload) => void;
        onCursor: (payload: CursorPayload) => void;
        onConnect: () => void;
        onDisconnect: () => void;
    }
): WebTransportClient {
    let ws: WebSocket | null = null;
    let connected = false;
    let videoQueue: VideoPayload[] = [];

    return {
        async connect(url: string) {
            ws = new WebSocket(url);
            ws.binaryType = 'arraybuffer';
            
            return new Promise<void>((resolve, reject) => {
                if (!ws) {
                    reject(new Error('WebSocket not initialized'));
                    return;
                }
                
                ws.onopen = () => {
                    connected = true;
                    handlers.onConnect();
                    resolve();
                };
                
                ws.onerror = (error) => {
                    reject(error);
                };
                
                ws.onclose = () => {
                    connected = false;
                    handlers.onDisconnect();
                };
                
                ws.onmessage = (event) => {
                    const message: ServerMessage = JSON.parse(event.data);
                    
                    switch (message.type) {
                        case 'video':
                            videoQueue.push(message.payload as VideoPayload);
                            break;
                        case 'cursor':
                            handlers.onCursor(message.payload as CursorPayload);
                            break;
                    }
                };
            });
        },
        
        close() {
            connected = false;
            ws?.close();
            ws = null;
        },
        
        async receiveVideo(): Promise<VideoPayload> {
            return new Promise((resolve) => {
                const checkQueue = () => {
                    if (videoQueue.length > 0) {
                        resolve(videoQueue.shift()!);
                    } else {
                        setTimeout(checkQueue, 1);
                    }
                };
                checkQueue();
            });
        },
        
        sendInput(input: InputPayload) {
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            
            const message: ClientMessage = {
                type: 'input',
                timestamp: performance.now(),
                payload: input,
            };
            
            ws.send(JSON.stringify(message));
        },
        
        isConnected() {
            return connected;
        },
    };
}

export default VideoPlayer;
