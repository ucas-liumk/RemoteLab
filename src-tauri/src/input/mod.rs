//! 输入处理模块 - Input Agent
//!
//! 职责：
//! 1. 接收客户端输入事件
//! 2. 本地预测与远程校正
//! 3. 输入同步

use std::collections::VecDeque;

/// 输入事件
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// 鼠标移动
    MouseMove { x: i32, y: i32 },
    /// 鼠标按下
    MouseDown { x: i32, y: i32, button: MouseButton },
    /// 鼠标释放
    MouseUp { x: i32, y: i32, button: MouseButton },
    /// 滚轮
    MouseWheel { x: i32, y: i32, delta: i32 },
    /// 键盘按下
    KeyDown { key_code: KeyCode, modifiers: Modifiers },
    /// 键盘释放
    KeyUp { key_code: KeyCode, modifiers: Modifiers },
}

/// 鼠标按钮
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Side(u8), // 侧键
}

/// 键盘按键代码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // 字母
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    // 数字
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    // 功能键
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    // 控制键
    Escape, Tab, CapsLock, Shift, Control, Alt, Meta,
    Space, Enter, Backspace,
    // 方向键
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    // 其他
    Insert, Delete, Home, End, PageUp, PageDown,
}

/// 修饰键状态
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

/// 预测输入事件
#[derive(Debug, Clone)]
pub struct PredictedEvent {
    /// 事件序列号
    pub sequence: u64,
    /// 预测时间戳
    pub predicted_timestamp_us: u64,
    /// 事件类型
    pub event: InputEvent,
    /// 预测位置 (用于鼠标)
    pub predicted_position: Option<(i32, i32)>,
}

/// 校正信息
#[derive(Debug, Clone)]
pub struct Correction {
    /// 序列号 (对应预测)
    pub sequence: u64,
    /// 实际位置
    pub actual_position: (i32, i32),
    /// 校正时间戳
    pub correction_timestamp_us: u64,
}

/// 输入预测器 trait
pub trait InputPredictor: Send + Sync {
    /// 添加实际输入事件
    fn add_event(&mut self, event: InputEvent, sequence: u64);
    
    /// 预测下一帧的输入状态
    fn predict(&self, delta_time_ms: f32) -> Option<PredictedEvent>;
    
    /// 应用校正
    fn apply_correction(&mut self, correction: Correction);
    
    /// 获取预测准确率
    fn get_accuracy(&self) -> f32;
}

/// 简单线性预测器
pub struct LinearPredictor {
    /// 历史事件
    history: VecDeque<(u64, InputEvent, u64)>, // (sequence, event, timestamp)
    /// 最大历史长度
    max_history: usize,
    /// 正确预测数
    correct_predictions: u64,
    /// 总预测数
    total_predictions: u64,
}

impl LinearPredictor {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_history,
            correct_predictions: 0,
            total_predictions: 0,
        }
    }
    
    /// 计算速度向量
    fn calculate_velocity(&self) -> Option<(f32, f32)> {
        if self.history.len() < 2 {
            return None;
        }
        
        let (seq1, event1, ts1) = self.history.get(self.history.len() - 2)?;
        let (seq2, event2, ts2) = self.history.get(self.history.len() - 1)?;
        
        match (event1, event2) {
            (InputEvent::MouseMove { x: x1, y: y1 }, InputEvent::MouseMove { x: x2, y: y2 }) => {
                let dt = (*ts2 as f32 - *ts1 as f32) / 1_000_000.0; // 转换为秒
                if dt > 0.0 {
                    let vx = (*x2 as f32 - *x1 as f32) / dt;
                    let vy = (*y2 as f32 - *y1 as f32) / dt;
                    Some((vx, vy))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl InputPredictor for LinearPredictor {
    fn add_event(&mut self, event: InputEvent, sequence: u64) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        
        self.history.push_back((sequence, event, timestamp));
        
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }
    
    fn predict(&self, delta_time_ms: f32) -> Option<PredictedEvent> {
        let velocity = self.calculate_velocity()?;
        
        let (_, last_event, timestamp) = self.history.back()?;
        
        match last_event {
            InputEvent::MouseMove { x, y } => {
                let predicted_x = (*x as f32 + velocity.0 * delta_time_ms / 1000.0) as i32;
                let predicted_y = (*y as f32 + velocity.1 * delta_time_ms / 1000.0) as i32;
                
                Some(PredictedEvent {
                    sequence: self.history.len() as u64 + 1,
                    predicted_timestamp_us: *timestamp + (delta_time_ms * 1000.0) as u64,
                    event: InputEvent::MouseMove { x: predicted_x, y: predicted_y },
                    predicted_position: Some((predicted_x, predicted_y)),
                })
            }
            _ => None,
        }
    }
    
    fn apply_correction(&mut self, correction: Correction) {
        // 更新预测准确率
        self.total_predictions += 1;
        
        // 检查预测是否准确
        if let Some((_, event, _)) = self.history.iter().find(|(s, _, _)| *s == correction.sequence) {
            if let InputEvent::MouseMove { x, y } = event {
                let dx = (*x - correction.actual_position.0).abs();
                let dy = (*y - correction.actual_position.1).abs();
                
                if dx <= 5 && dy <= 5 {
                    self.correct_predictions += 1;
                }
            }
        }
    }
    
    fn get_accuracy(&self) -> f32 {
        if self.total_predictions == 0 {
            return 0.0;
        }
        self.correct_predictions as f32 / self.total_predictions as f32
    }
}

impl Default for LinearPredictor {
    fn default() -> Self {
        Self::new(10)
    }
}

/// 输入处理器
pub struct InputHandler {
    predictor: Box<dyn InputPredictor>,
    pending_events: VecDeque<InputEvent>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            predictor: Box::new(LinearPredictor::default()),
            pending_events: VecDeque::new(),
        }
    }
    
    /// 处理输入事件
    pub fn handle_event(&mut self, event: InputEvent, sequence: u64) {
        self.predictor.add_event(event.clone(), sequence);
        self.pending_events.push_back(event);
    }
    
    /// 获取待处理的输入
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.pending_events.pop_front()
    }
    
    /// 预测下一帧输入
    pub fn predict(&self, delta_time_ms: f32) -> Option<PredictedEvent> {
        self.predictor.predict(delta_time_ms)
    }
    
    /// 应用校正
    pub fn apply_correction(&mut self, correction: Correction) {
        self.predictor.apply_correction(correction);
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建输入处理器
pub fn create_input_handler() -> InputHandler {
    InputHandler::new()
}
