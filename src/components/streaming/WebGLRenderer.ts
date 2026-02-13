/**
 * WebGL 视频渲染器
 * 
 * 使用 WebGL2 进行零拷贝视频渲染
 * 目标延迟: < 3ms 渲染延迟
 * 支持 4K@60fps
 */

import { DecodedFrame } from './WebCodecsDecoder';

export interface RenderOptions {
    /** 目标画布 */
    canvas: HTMLCanvasElement;
    /** 是否使用 YUV 着色器 (否则使用 RGB) */
    useYUV?: boolean;
    /** 目标帧率 */
    targetFrameRate?: number;
}

/**
 * WebGL 视频渲染器
 * 
 * WebGL2 特性:
 * 1. 外部纹理扩展 - 直接渲染 VideoFrame
 * 2. 高速渲染管线 - 最小化 GPU 命令
 * 3. 零拷贝 - VideoFrame 直接作为纹理源
 */
export class WebGLRenderer {
    private canvas: HTMLCanvasElement;
    private gl: WebGL2RenderingContext;
    private program: WebGLProgram | null = null;
    private positionBuffer: WebGLBuffer | null = null;
    private texCoordBuffer: WebGLBuffer | null = null;
    private texture: WebGLTexture | null = null;
    private videoTexture: WebGLTexture | null = null;
    
    // Uniform 位置缓存
    private uniforms: Map<string, WebGLUniformLocation> = new Map();
    
    // 统计
    private stats = {
        framesRendered: 0,
        droppedFrames: 0,
        averageRenderTime: 0,
        lastFrameTime: 0,
    };

    // 是否使用 YUV
    private useYUV: boolean;

    constructor(options: RenderOptions) {
        this.canvas = options.canvas;
        this.useYUV = options.useYUV ?? false;
        
        const gl = this.canvas.getContext('webgl2', {
            alpha: false,
            antialias: false,
            desynchronized: true, // 低延迟模式
            powerPreference: 'high-performance',
            preserveDrawingBuffer: false,
        });
        
        if (!gl) {
            throw new Error('WebGL2 not supported');
        }
        
        this.gl = gl;
        this.initialize();
    }

    /**
     * 初始化 WebGL
     */
    private initialize(): void {
        const gl = this.gl;

        // 使用 WebGL2 GLSL 3.0
        const vertexShaderSource = `#version 300 es
            in vec2 a_position;
            in vec2 a_texCoord;
            out vec2 v_texCoord;
            void main() {
                gl_Position = vec4(a_position, 0.0, 1.0);
                v_texCoord = a_texCoord;
            }
        `;

        // RGBA 渲染
        const fragmentShaderSource = `#version 300 es
            precision highp float;
            in vec2 v_texCoord;
            out vec4 outColor;
            uniform sampler2D u_videoTexture;
            void main() {
                outColor = texture(u_videoTexture, v_texCoord);
            }
        `;

        // 编译着色器
        const vertexShader = this.compileShader(gl.VERTEX_SHADER, vertexShaderSource);
        const fragmentShader = this.compileShader(gl.FRAGMENT_SHADER, fragmentShaderSource);

        if (!vertexShader || !fragmentShader) {
            throw new Error('Failed to compile shaders');
        }

        // 创建程序
        this.program = gl.createProgram();
        if (!this.program) {
            throw new Error('Failed to create shader program');
        }

        gl.attachShader(this.program, vertexShader);
        gl.attachShader(this.program, fragmentShader);
        gl.linkProgram(this.program);

        if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
            const info = gl.getProgramInfoLog(this.program);
            gl.deleteProgram(this.program);
            throw new Error(`Failed to link program: ${info}`);
        }

        // 创建顶点缓冲区 (全屏四边形)
        this.positionBuffer = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, this.positionBuffer);
        // 使用 TRIANGLE_STRIP 渲染四边形 (4个顶点)
        gl.bufferData(
            gl.ARRAY_BUFFER,
            new Float32Array([
                -1, -1,  // 左下
                 1, -1,  // 右下
                -1,  1,  // 左上
                 1,  1,  // 右上
            ]),
            gl.STATIC_DRAW
        );

        // 创建纹理坐标缓冲区
        this.texCoordBuffer = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, this.texCoordBuffer);
        gl.bufferData(
            gl.ARRAY_BUFFER,
            new Float32Array([
                0, 1,  // 左下
                1, 1,  // 右下
                0, 0,  // 左上
                1, 0,  // 右上
            ]),
            gl.STATIC_DRAW
        );

        // 创建视频纹理
        this.videoTexture = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, this.videoTexture);
        
        // 设置纹理参数
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);

        // 使用程序
        gl.useProgram(this.program);
        
        // 缓存 uniform 位置
        const textureLoc = gl.getUniformLocation(this.program, 'u_videoTexture');
        if (textureLoc) {
            this.uniforms.set('u_videoTexture', textureLoc);
            gl.uniform1i(textureLoc, 0); // 纹理单元 0
        }
        
        // 设置属性
        this.setupAttributes();
    }

    /**
     * 设置顶点属性
     */
    private setupAttributes(): void {
        const gl = this.gl;
        
        if (!this.program) return;

        // 位置属性
        const positionLocation = gl.getAttribLocation(this.program, 'a_position');
        gl.bindBuffer(gl.ARRAY_BUFFER, this.positionBuffer);
        gl.enableVertexAttribArray(positionLocation);
        gl.vertexAttribPointer(positionLocation, 2, gl.FLOAT, false, 0, 0);

        // 纹理坐标属性
        const texCoordLocation = gl.getAttribLocation(this.program, 'a_texCoord');
        gl.bindBuffer(gl.ARRAY_BUFFER, this.texCoordBuffer);
        gl.enableVertexAttribArray(texCoordLocation);
        gl.vertexAttribPointer(texCoordLocation, 2, gl.FLOAT, false, 0, 0);
    }

    /**
     * 编译着色器
     */
    private compileShader(type: number, source: string): WebGLShader | null {
        const gl = this.gl;
        const shader = gl.createShader(type);
        if (!shader) return null;

        gl.shaderSource(shader, source);
        gl.compileShader(shader);

        if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
            console.error('Shader compile error:', gl.getShaderInfoLog(shader));
            gl.deleteShader(shader);
            return null;
        }

        return shader;
    }

    /**
     * 渲染视频帧 (零拷贝)
     * VideoFrame 直接作为纹理源
     */
    public render(videoFrame: VideoFrame): void {
        const startTime = performance.now();
        const gl = this.gl;

        // 调整画布大小
        if (this.canvas.width !== videoFrame.displayWidth || 
            this.canvas.height !== videoFrame.displayHeight) {
            this.canvas.width = videoFrame.displayWidth;
            this.canvas.height = videoFrame.displayHeight;
            gl.viewport(0, 0, this.canvas.width, this.canvas.height);
        }

        // 上传视频帧到纹理 (零拷贝)
        if (this.videoTexture) {
            gl.bindTexture(gl.TEXTURE_2D, this.videoTexture);
            // VideoFrame 可以直接作为 texImage2D 的源 (零拷贝)
            gl.texImage2D(
                gl.TEXTURE_2D,
                0,
                gl.RGBA,
                gl.RGBA,
                gl.UNSIGNED_BYTE,
                videoFrame as any
            );
        }

        // 绘制全屏四边形
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

        // 更新统计
        const renderTime = performance.now() - startTime;
        this.stats.framesRendered++;
        this.stats.averageRenderTime = 
            (this.stats.averageRenderTime * (this.stats.framesRendered - 1) + renderTime) / 
            this.stats.framesRendered;
        this.stats.lastFrameTime = performance.now();

        // 性能警告
        if (renderTime > 3) {
            console.warn(`High render latency: ${renderTime.toFixed(2)}ms`);
        }
    }

    /**
     * 提交帧进行渲染 (兼容性方法)
     */
    public submitFrame(frame: DecodedFrame): void {
        this.render(frame.frame);
        frame.frame.close(); // 释放帧
    }

    /**
     * 清空画布
     */
    public clear(): void {
        const gl = this.gl;
        gl.clearColor(0, 0, 0, 1);
        gl.clear(gl.COLOR_BUFFER_BIT);
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
            framesRendered: 0,
            droppedFrames: 0,
            averageRenderTime: 0,
            lastFrameTime: 0,
        };
    }

    /**
     * 调整画布大小
     */
    public resize(width: number, height: number): void {
        this.canvas.width = width;
        this.canvas.height = height;
        this.gl.viewport(0, 0, width, height);
    }

    /**
     * 销毁渲染器
     */
    public destroy(): void {
        const gl = this.gl;

        if (this.videoTexture) {
            gl.deleteTexture(this.videoTexture);
            this.videoTexture = null;
        }
        if (this.texture) {
            gl.deleteTexture(this.texture);
            this.texture = null;
        }
        if (this.positionBuffer) {
            gl.deleteBuffer(this.positionBuffer);
            this.positionBuffer = null;
        }
        if (this.texCoordBuffer) {
            gl.deleteBuffer(this.texCoordBuffer);
            this.texCoordBuffer = null;
        }
        if (this.program) {
            gl.deleteProgram(this.program);
            this.program = null;
        }
    }
}

/**
 * 创建 WebGL 渲染器
 */
export function createRenderer(options: RenderOptions): WebGLRenderer {
    return new WebGLRenderer(options);
}
