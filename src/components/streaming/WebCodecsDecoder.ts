/**
 * WebCodecs 视频解码器
 * 
 * 使用 WebCodecs API 进行硬件加速视频解码
 * 目标延迟: < 5ms 解码延迟
 * 支持 4K@60fps
 */

import { VideoPayload } from './protocol';
import { WebGLRenderer } from './WebGLRenderer';

export interface DecodedFrame {
    /** 视频帧 */
    frame: VideoFrame;
    /** 帧 ID */
    frameId: number;
    /** 解码时间戳 */
    decodeTimestamp: number;
    /** 渲染时间戳 */
    renderTimestamp: number;
}

export type CodecType = 'h264' | 'hevc' | 'av1';

export interface DecoderConfig {
    codec: CodecType;
    width?: number;
    height?: number;
    /** 是否使用硬件加速 */
    hardwareAcceleration?: 'prefer-hardware' | 'prefer-software';
    /** 是否优化延迟 */
    optimizeForLatency?: boolean;
}

/**
 * WebCodecs 视频解码器
 * 
 * 实现要点:
 * 1. 立即渲染，不缓冲 - 降低延迟
 * 2. 硬件加速优先 - GPU 解码
 * 3. 零拷贝渲染 - VideoFrame 直接传 WebGL
 * 4. 帧立即释放 - 避免内存堆积
 */
export class WebCodecsDecoder {
    private decoder: VideoDecoder | null = null;
    private config: DecoderConfig;
    private webglRenderer: WebGLRenderer | null = null;
    private canvas: HTMLCanvasElement | null = null;
    private ctx: CanvasRenderingContext2D | null = null;
    
    // 帧队列 (最小缓冲)
    private frameQueue: VideoFrame[] = [];
    private maxQueueLength = 1; // 关键: 最小缓冲
    
    // 回调
    private frameCallbacks: ((frame: DecodedFrame) => void)[] = [];
    private errorCallbacks: ((error: Error) => void)[] = [];
    
    // 统计
    private stats = {
        framesDecoded: 0,
        framesDropped: 0,
        decodeErrors: 0,
        averageDecodeTime: 0,
        lastFrameTime: 0,
    };

    constructor(config: DecoderConfig) {
        this.config = {
            hardwareAcceleration: 'prefer-hardware',
            optimizeForLatency: true,
            ...config,
        };
    }

    /**
     * 初始化解码器
     */
    public async init(canvas?: HTMLCanvasElement): Promise<void> {
        if (!('VideoDecoder' in window)) {
            throw new Error('WebCodecs API not supported');
        }

        if (canvas) {
            this.canvas = canvas;
            // 尝试 WebGL 渲染
            this.initWebGL();
            
            // WebGL 失败则回退到 Canvas 2D
            if (!this.webglRenderer) {
                this.ctx = canvas.getContext('2d', {
                    alpha: false,
                    desynchronized: true, // 低延迟模式
                });
            }
        }

        const codecString = this.getCodecString(this.config.codec);

        // 检查是否支持硬件解码
        const support = await VideoDecoder.isConfigSupported({
            codec: codecString,
            hardwareAcceleration: this.config.hardwareAcceleration,
        });

        if (!support.supported) {
            console.warn('Hardware decoding not supported, falling back to software');
        }

        this.decoder = new VideoDecoder({
            output: this.handleDecodedFrame.bind(this),
            error: this.handleError.bind(this),
        });

        this.decoder.configure({
            codec: codecString,
            hardwareAcceleration: this.config.hardwareAcceleration,
            optimizeForLatency: this.config.optimizeForLatency,
        });
    }

    /**
     * 初始化 WebGL 渲染器
     */
    private initWebGL(): void {
        if (!this.canvas) return;
        
        try {
            this.webglRenderer = new WebGLRenderer({
                canvas: this.canvas,
                useYUV: false,
                targetFrameRate: 60,
            });
        } catch (error) {
            console.warn('WebGL initialization failed, falling back to Canvas 2D:', error);
            this.webglRenderer = null;
        }
    }

    /**
     * 获取 codec 字符串
     */
    private getCodecString(codec: CodecType): string {
        switch (codec) {
            case 'h264':
                // H.264 High profile, level 3.0
                return 'avc1.64001e';
            case 'hevc':
                // HEVC Main profile, level 4.0
                return 'hev1.1.6.L120.90';
            case 'av1':
                // AV1 Main profile, level 4.0
                return 'av01.0.04M.08';
            default:
                throw new Error(`Unsupported codec: ${codec}`);
        }
    }

    /**
     * 解码视频帧
     * 立即解码，不缓冲
     */
    public decode(payload: VideoPayload): void {
        if (!this.decoder) {
            throw new Error('Decoder not initialized');
        }

        const startTime = performance.now();

        // 创建 EncodedVideoChunk
        const chunk = new EncodedVideoChunk({
            type: payload.frameType === 'idr' ? 'key' : 'delta',
            timestamp: payload.encodeTimestamp,
            data: payload.data,
        });

        try {
            this.decoder.decode(chunk);
        } catch (error) {
            console.error('Decode error:', error);
            this.stats.decodeErrors++;
            this.stats.framesDropped++;
        }

        const decodeTime = performance.now() - startTime;
        this.updateDecodeTimeStats(decodeTime);
    }

    /**
     * 处理解码后的帧
     * 立即渲染，不缓冲
     */
    private handleDecodedFrame(frame: VideoFrame): void {
        const now = performance.now();

        this.stats.framesDecoded++;

        const decodedFrame: DecodedFrame = {
            frame,
            frameId: this.stats.framesDecoded,
            decodeTimestamp: now,
            renderTimestamp: frame.timestamp ?? now,
        };

        // 立即渲染 (不缓冲)
        this.render(frame);
        
        // 立即释放帧
        frame.close();

        this.stats.lastFrameTime = now;

        // 通知所有回调
        this.frameCallbacks.forEach(cb => cb(decodedFrame));
    }

    /**
     * 渲染帧
     * 零拷贝渲染
     */
    private render(frame: VideoFrame): void {
        const startTime = performance.now();

        if (this.webglRenderer) {
            // WebGL 零拷贝渲染
            this.webglRenderer.render(frame);
        } else if (this.ctx && this.canvas) {
            // Canvas 2D 回退
            // 调整画布大小
            if (this.canvas.width !== frame.displayWidth || 
                this.canvas.height !== frame.displayHeight) {
                this.canvas.width = frame.displayWidth;
                this.canvas.height = frame.displayHeight;
            }
            this.ctx.drawImage(frame, 0, 0);
        }

        const renderTime = performance.now() - startTime;
        // 渲染延迟应该 < 3ms
        if (renderTime > 3) {
            console.warn(`High render latency: ${renderTime.toFixed(2)}ms`);
        }
    }

    /**
     * 处理解码错误
     */
    private handleError(error: Error): void {
        console.error('Decoder error:', error);
        this.stats.decodeErrors++;
        this.errorCallbacks.forEach(cb => cb(error));
    }

    /**
     * 更新解码时间统计
     */
    private updateDecodeTimeStats(time: number): void {
        this.stats.averageDecodeTime = 
            (this.stats.averageDecodeTime * this.stats.framesDecoded + time) / 
            (this.stats.framesDecoded + 1);
    }

    /**
     * 注册帧回调
     */
    public onFrame(callback: (frame: DecodedFrame) => void): void {
        this.frameCallbacks.push(callback);
    }

    /**
     * 注册错误回调
     */
    public onError(callback: (error: Error) => void): void {
        this.errorCallbacks.push(callback);
    }

    /**
     * 刷新解码器
     */
    public async flush(): Promise<void> {
        if (this.decoder) {
            await this.decoder.flush();
        }
    }

    /**
     * 关闭解码器
     */
    public close(): void {
        if (this.decoder) {
            this.decoder.close();
            this.decoder = null;
        }
        
        // 清理队列
        this.frameQueue.forEach(f => f.close());
        this.frameQueue = [];
        
        // 清理渲染器
        this.webglRenderer?.destroy();
        this.webglRenderer = null;
    }

    /**
     * 获取统计信息
     */
    public getStats() {
        return { ...this.stats };
    }

    /**
     * 重置统计
     */
    public resetStats(): void {
        this.stats = {
            framesDecoded: 0,
            framesDropped: 0,
            decodeErrors: 0,
            averageDecodeTime: 0,
            lastFrameTime: 0,
        };
    }
}

/**
 * 简化的解码 API
 * 用于直接解码和渲染
 */
export async function createDecoder(
    config: DecoderConfig, 
    canvas?: HTMLCanvasElement
): Promise<WebCodecsDecoder> {
    const decoder = new WebCodecsDecoder(config);
    await decoder.init(canvas);
    return decoder;
}
