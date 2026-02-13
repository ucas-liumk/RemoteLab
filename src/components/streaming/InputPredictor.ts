/**
 * 输入预测器 (Parsec 技术)
 * 
 * 实现客户端输入预测算法，降低感知延迟
 * 目标: 输入响应 < 16ms (1帧)
 * 
 * 技术要点:
 * 1. 二阶预测 (位置 + 速度 + 加速度)
 * 2. 本地立即渲染预测位置
 * 3. 服务器校正平滑处理
 */

export interface InputEvent {
    type: 'mouse-move' | 'mouse-down' | 'mouse-up' | 'key-down' | 'key-up' | 'mouse-wheel';
    x?: number;
    y?: number;
    key?: string;
    button?: number;
    deltaY?: number;
    timestamp: number;
    sequence: number;
}

export interface MousePrediction {
    /** 实际位置 */
    actual: { x: number; y: number };
    /** 预测位置 */
    predicted: { x: number; y: number };
    /** 时间戳 */
    timestamp: number;
    /** 置信度 (0-1) */
    confidence: number;
}

export interface KeyPrediction {
    key: string;
    display?: string;
    timestamp: number;
}

export interface CorrectionData {
    sequence: number;
    actualX: number;
    actualY: number;
    serverTimestamp: number;
}

export interface PredictedInput {
    event: InputEvent;
    predictedX?: number;
    predictedY?: number;
    confidence: number;
}

/**
 * 输入预测器
 * 
 * Parsec 技术实现:
 * - 预测时间窗口: 16ms (1帧)
 * - 二阶预测: 位置 + 速度 + 加速度
 * - 平滑校正: 分3帧修正误差
 */
export class InputPredictor {
    // 状态
    private lastMousePos = { x: 0, y: 0 };
    private velocity = { x: 0, y: 0 };
    private acceleration = { x: 0, y: 0 };
    private lastTimestamp = 0;
    
    // 拖动状态
    private isDragging = false;
    private dragStartPos = { x: 0, y: 0 };
    
    // 预测参数
    private readonly PREDICTION_HORIZON = 16; // 16ms = 1帧 @ 60fps
    
    // 历史记录
    private history: InputEvent[] = [];
    private maxHistory = 10;
    private sequence = 0;
    
    // 平滑系数
    private alpha = 0.3;
    
    // 是否启用预测
    private enabled = true;
    
    // 文本输入状态
    private textInputFocused = false;
    
    // 统计
    private predictions = 0;
    private correctPredictions = 0;
    
    // 校正回调
    private correctionCallbacks: ((error: { x: number; y: number }) => void)[] = [];

    /**
     * 鼠标移动事件处理
     * 二阶预测算法
     */
    public onMouseMove(x: number, y: number): MousePrediction {
        const now = performance.now();
        const dt = Math.max(now - this.lastTimestamp, 1); // 避免除以0
        
        // 保存事件历史
        const event: InputEvent = {
            type: 'mouse-move',
            x,
            y,
            timestamp: now,
            sequence: ++this.sequence,
        };
        this.addEvent(event);
        
        // 计算新速度
        const newVelocity = {
            x: (x - this.lastMousePos.x) / dt,
            y: (y - this.lastMousePos.y) / dt,
        };
        
        // 计算加速度 (速度变化率)
        this.acceleration = {
            x: (newVelocity.x - this.velocity.x) / dt,
            y: (newVelocity.y - this.velocity.y) / dt,
        };
        
        // 平滑速度 (指数平滑)
        this.velocity = {
            x: this.alpha * newVelocity.x + (1 - this.alpha) * this.velocity.x,
            y: this.alpha * newVelocity.y + (1 - this.alpha) * this.velocity.y,
        };
        
        // 二阶预测: p = p0 + v*t + 0.5*a*t^2
        const predictedX = x + 
            this.velocity.x * this.PREDICTION_HORIZON +
            0.5 * this.acceleration.x * Math.pow(this.PREDICTION_HORIZON, 2);
        
        const predictedY = y + 
            this.velocity.y * this.PREDICTION_HORIZON +
            0.5 * this.acceleration.y * Math.pow(this.PREDICTION_HORIZON, 2);
        
        // 更新状态
        this.lastMousePos = { x, y };
        this.lastTimestamp = now;
        this.predictions++;
        
        // 计算置信度
        const confidence = this.calculateConfidence();
        
        return {
            actual: { x, y },
            predicted: { 
                x: Math.round(predictedX), 
                y: Math.round(predictedY) 
            },
            timestamp: now,
            confidence,
        };
    }

    /**
     * 服务器位置校正
     * 平滑处理预测误差
     */
    public onServerCorrection(serverPos: { x: number; y: number }): void {
        const error = {
            x: serverPos.x - this.lastMousePos.x,
            y: serverPos.y - this.lastMousePos.y,
        };
        
        const errorMagnitude = Math.sqrt(error.x ** 2 + error.y ** 2);
        
        // 误差在可接受范围内
        if (errorMagnitude < 5) {
            this.correctPredictions++;
        }
        
        if (errorMagnitude > 10) {
            // 误差过大，需要平滑修正
            this.smoothCorrection(error);
            
            // 调整平滑系数
            this.alpha = Math.max(0.1, this.alpha * 0.9);
        } else if (errorMagnitude < 3) {
            // 误差小，可以减小平滑
            this.alpha = Math.min(0.5, this.alpha * 1.05);
        }
        
        // 基于实际位置重置速度估计
        const dt = 16; // 假设一帧时间
        this.velocity = {
            x: (serverPos.x - this.lastMousePos.x) / dt,
            y: (serverPos.y - this.lastMousePos.y) / dt,
        };
        
        this.lastMousePos = serverPos;
    }

    /**
     * 平滑校正
     * 分3帧平滑修正误差，避免突兀跳变
     */
    private smoothCorrection(error: { x: number; y: number }): void {
        const steps = 3;
        const baseDelay = 5; // ms
        
        for (let i = 1; i <= steps; i++) {
            setTimeout(() => {
                const factor = i / steps;
                this.emitCorrection({
                    x: error.x * factor,
                    y: error.y * factor,
                });
            }, i * baseDelay);
        }
    }

    /**
     * 触发校正事件
     */
    private emitCorrection(error: { x: number; y: number }): void {
        this.correctionCallbacks.forEach(cb => cb(error));
    }

    /**
     * 注册校正回调
     */
    public onCorrection(callback: (error: { x: number; y: number }) => void): void {
        this.correctionCallbacks.push(callback);
    }

    /**
     * 键盘按下事件
     * 文本输入本地预测
     */
    public onKeyDown(key: string): KeyPrediction {
        const now = performance.now();
        
        // 如果是文本输入，立即显示字符
        if (this.isTextInputFocused()) {
            return {
                key,
                display: this.mapKeyToChar(key),
                timestamp: now,
            };
        }
        
        return { key, timestamp: now };
    }

    /**
     * 键盘释放事件
     */
    public onKeyUp(key: string): KeyPrediction {
        return { key, timestamp: performance.now() };
    }

    /**
     * 鼠标按下
     */
    public onMouseDown(x: number, y: number, button: number): InputEvent {
        this.isDragging = true;
        this.dragStartPos = { x, y };
        
        return {
            type: 'mouse-down',
            x,
            y,
            button,
            timestamp: performance.now(),
            sequence: ++this.sequence,
        };
    }

    /**
     * 鼠标释放
     */
    public onMouseUp(x: number, y: number, button: number): InputEvent {
        this.isDragging = false;
        
        return {
            type: 'mouse-up',
            x,
            y,
            button,
            timestamp: performance.now(),
            sequence: ++this.sequence,
        };
    }

    /**
     * 鼠标滚轮
     */
    public onMouseWheel(deltaY: number): InputEvent {
        return {
            type: 'mouse-wheel',
            deltaY,
            timestamp: performance.now(),
            sequence: ++this.sequence,
        };
    }

    /**
     * 添加输入事件到历史
     */
    public addEvent(event: InputEvent): void {
        if (!event.sequence) {
            event.sequence = ++this.sequence;
        }
        
        this.history.push(event);
        
        // 保持历史长度
        if (this.history.length > this.maxHistory) {
            this.history.shift();
        }
    }

    /**
     * 预测下一帧的输入状态
     * @param deltaTime 预测时间间隔 (ms)
     */
    public predict(deltaTime: number): PredictedInput | null {
        if (!this.enabled || this.history.length === 0) {
            return null;
        }

        const lastEvent = this.history[this.history.length - 1];

        // 只预测鼠标移动
        if (lastEvent.type !== 'mouse-move' || 
            lastEvent.x === undefined || 
            lastEvent.y === undefined) {
            return null;
        }

        // 使用二阶预测
        const predictedX = lastEvent.x + 
            this.velocity.x * deltaTime +
            0.5 * this.acceleration.x * Math.pow(deltaTime, 2);
        
        const predictedY = lastEvent.y + 
            this.velocity.y * deltaTime +
            0.5 * this.acceleration.y * Math.pow(deltaTime, 2);

        // 计算置信度
        const confidence = this.calculateConfidence();

        this.predictions++;

        return {
            event: {
                type: 'mouse-move',
                x: predictedX,
                y: predictedY,
                timestamp: lastEvent.timestamp + deltaTime,
                sequence: lastEvent.sequence,
            },
            predictedX,
            predictedY,
            confidence,
        };
    }

    /**
     * 计算预测置信度
     * 基于速度和加速度的一致性
     */
    private calculateConfidence(): number {
        if (this.history.length < 3) return 0.5;

        // 基于速度变化一致性计算置信度
        let velocityVariance = 0;
        let avgVx = 0;
        let avgVy = 0;

        for (let i = 1; i < this.history.length; i++) {
            const curr = this.history[i];
            const prev = this.history[i - 1];

            if (curr.type === 'mouse-move' && prev.type === 'mouse-move' &&
                curr.x !== undefined && curr.y !== undefined &&
                prev.x !== undefined && prev.y !== undefined) {
                const dt = curr.timestamp - prev.timestamp;
                if (dt > 0) {
                    avgVx += (curr.x - prev.x) / dt;
                    avgVy += (curr.y - prev.y) / dt;
                }
            }
        }

        avgVx /= this.history.length - 1;
        avgVy /= this.history.length - 1;

        // 计算方差
        for (let i = 1; i < this.history.length; i++) {
            const curr = this.history[i];
            const prev = this.history[i - 1];

            if (curr.type === 'mouse-move' && prev.type === 'mouse-move' &&
                curr.x !== undefined && curr.y !== undefined &&
                prev.x !== undefined && prev.y !== undefined) {
                const dt = curr.timestamp - prev.timestamp;
                if (dt > 0) {
                    const vx = (curr.x - prev.x) / dt;
                    const vy = (curr.y - prev.y) / dt;
                    velocityVariance += Math.pow(vx - avgVx, 2) + Math.pow(vy - avgVy, 2);
                }
            }
        }

        velocityVariance /= this.history.length - 1;

        // 方差越小，置信度越高
        const confidence = Math.max(0, Math.min(1, 1 - velocityVariance / 100));
        return confidence;
    }

    /**
     * 应用校正 (兼容性方法)
     */
    public applyCorrection(correction: CorrectionData): void {
        this.onServerCorrection({ x: correction.actualX, y: correction.actualY });
    }

    /**
     * 获取预测准确率
     */
    public getAccuracy(): number {
        if (this.predictions === 0) return 0;
        return this.correctPredictions / this.predictions;
    }

    /**
     * 检查是否在文本输入模式
     */
    private isTextInputFocused(): boolean {
        return this.textInputFocused;
    }

    /**
     * 设置文本输入焦点状态
     */
    public setTextInputFocused(focused: boolean): void {
        this.textInputFocused = focused;
    }

    /**
     * 映射按键到字符
     */
    private mapKeyToChar(key: string): string {
        // 简单映射，实际应用中需要更完整的映射表
        const keyMap: Record<string, string> = {
            'Enter': '\n',
            'Tab': '\t',
            'Space': ' ',
        };
        
        if (key.length === 1) {
            return key;
        }
        
        return keyMap[key] || '';
    }

    /**
     * 获取统计信息
     */
    public getStats() {
        return {
            predictions: this.predictions,
            correctPredictions: this.correctPredictions,
            accuracy: this.getAccuracy(),
            velocityX: this.velocity.x,
            velocityY: this.velocity.y,
            accelerationX: this.acceleration.x,
            accelerationY: this.acceleration.y,
            alpha: this.alpha,
        };
    }

    /**
     * 启用/禁用预测
     */
    public setEnabled(enabled: boolean): void {
        this.enabled = enabled;
    }

    /**
     * 是否启用
     */
    public isEnabled(): boolean {
        return this.enabled;
    }

    /**
     * 重置状态
     */
    public reset(): void {
        this.history = [];
        this.predictions = 0;
        this.correctPredictions = 0;
        this.velocity = { x: 0, y: 0 };
        this.acceleration = { x: 0, y: 0 };
        this.sequence = 0;
        this.lastMousePos = { x: 0, y: 0 };
        this.lastTimestamp = 0;
        this.isDragging = false;
    }
}

/**
 * 创建预测器实例
 */
export function createPredictor(enabled = true): InputPredictor {
    const predictor = new InputPredictor();
    predictor.setEnabled(enabled);
    return predictor;
}
