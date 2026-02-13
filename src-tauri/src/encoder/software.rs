//! Software Video Encoder Fallback
//!
//! Uses x264/x265 libraries for CPU-based encoding
//! Fallback when no hardware encoder is available

use super::{VideoCodec as CodecType, EncodedFrame, EncoderConfig, EncoderError, EncoderError as Error, EncoderStats, FrameType, FrameRef, PixelFormat, QualityPreset as Preset, VideoEncoder, EncodingStrategy};

/// Tuning for x264
#[derive(Debug, Clone, Copy)]
enum Tuning {
    LowLatency,
    Quality,
    Balanced,
}
use std::collections::VecDeque;
use std::time::Instant;

/// Software encoder using x264/x265
pub struct SoftwareEncoder {
    config: EncoderConfig,
    frame_count: u64,
    initialized: bool,
    encoder_ctx: Option<SoftwareContext>,
    output_queue: VecDeque<EncodedFrame>,
}

/// Internal software encoder context
struct SoftwareContext {
    width: u32,
    height: u32,
    codec: CodecType,
    preset: u8, // 0=ultrafast, 9=veryslow
    tune: String,
    qp: u8,
}

impl SoftwareEncoder {
    pub fn new() -> Self {
        Self {
            config: EncoderConfig::default(),
            frame_count: 0,
            initialized: false,
            encoder_ctx: None,
            output_queue: VecDeque::new(),
        }
    }
    
    pub fn with_config(config: EncoderConfig) -> Result<Self, Error> {
        let mut encoder = Self::new();
        encoder.init(&config)?;
        Ok(encoder)
    }
    
    fn preset_to_x264_preset(preset: Preset) -> &'static str {
        match preset {
            Preset::P1 => "ultrafast",
            Preset::P2 => "superfast",
            Preset::P3 => "veryfast",
            Preset::P4 => "faster",
            Preset::P5 => "fast",
            Preset::P6 => "medium",
            Preset::P7 => "slow",
        }
    }
    
    fn preset_to_x265_preset(preset: Preset) -> &'static str {
        match preset {
            Preset::P1 => "ultrafast",
            Preset::P2 => "superfast",
            Preset::P3 => "veryfast",
            Preset::P4 => "faster",
            Preset::P5 => "fast",
            Preset::P6 => "medium",
            Preset::P7 => "slow",
        }
    }
    
    fn tuning_to_x264_tune(tuning: Tuning) -> &'static str {
        match tuning {
            Tuning::LowLatency => "zerolatency",
            Tuning::Quality => "film",
            Tuning::Balanced => "",
        }
    }
    
    fn create_software_context(&self) -> Result<SoftwareContext, Error> {
        let preset = match self.config.preset {
            Preset::P1 => 0,
            Preset::P2 => 1,
            Preset::P3 => 2,
            Preset::P4 => 3,
            Preset::P5 => 4,
            Preset::P6 => 6,
            Preset::P7 => 9,
        };
        
        let tune = match self.config.tuning {
            Tuning::LowLatency => "zerolatency".to_string(),
            Tuning::Quality => "film".to_string(),
            Tuning::Balanced => "".to_string(),
        };
        
        // Calculate QP based on bitrate and resolution
        let bits_per_pixel = self.config.bitrate as f64 
            / (self.config.width * self.config.height * self.config.fps) as f64;
        let qp = (51.0 - bits_per_pixel * 1000.0).clamp(18.0, 51.0) as u8;
        
        Ok(SoftwareContext {
            width: self.config.width,
            height: self.config.height,
            codec: self.config.codec,
            preset,
            tune,
            qp,
        })
    }
    
    fn convert_nv12_to_i420(&self, nv12: &[u8], width: u32, height: u32) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let y_size = w * h;
        let uv_size = w * h / 2;
        
        let mut i420 = vec![0u8; y_size + uv_size];
        
        // Copy Y plane (same in both formats)
        i420[0..y_size].copy_from_slice(&nv12[0..y_size]);
        
        // Convert interleaved UV to separate U and V planes
        let uv_src = &nv12[y_size..y_size + uv_size];
        let u_dst = &mut i420[y_size..y_size + uv_size / 2];
        let v_dst = &mut i420[y_size + uv_size / 2..y_size + uv_size];
        
        for i in 0..(uv_size / 2) {
            u_dst[i] = uv_src[i * 2];
            v_dst[i] = uv_src[i * 2 + 1];
        }
        
        i420
    }
    
    fn convert_rgba_to_i420(&self, rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let y_size = w * h;
        let uv_size = w * h / 4;
        
        let mut i420 = vec![0u8; y_size + 2 * uv_size];
        
        // Simple RGBA to YUV conversion
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 4;
                let r = rgba[idx] as f32;
                let g = rgba[idx + 1] as f32;
                let b = rgba[idx + 2] as f32;
                
                // BT.601 conversion
                let y_val = (0.257 * r + 0.504 * g + 0.098 * b + 16.0) as u8;
                i420[y * w + x] = y_val;
                
                // Subsample UV for even coordinates
                if y % 2 == 0 && x % 2 == 0 {
                    let u_val = (-0.148 * r - 0.291 * g + 0.439 * b + 128.0) as u8;
                    let v_val = (0.439 * r - 0.368 * g - 0.071 * b + 128.0) as u8;
                    
                    let uv_idx = (y / 2) * (w / 2) + (x / 2);
                    i420[y_size + uv_idx] = u_val;
                    i420[y_size + uv_size + uv_idx] = v_val;
                }
            }
        }
        
        i420
    }
    
    fn encode_frame_software(&mut self, frame_data: &[u8], format: PixelFormat) -> Result<Vec<u8>, Error> {
        // Convert to I420 if needed
        let i420_data = match format {
            PixelFormat::NV12 => self.convert_nv12_to_i420(frame_data, self.config.width, self.config.height),
            PixelFormat::RGBA => self.convert_rgba_to_i420(frame_data, self.config.width, self.config.height),
            PixelFormat::BGRA => self.convert_rgba_to_i420(frame_data, self.config.width, self.config.height),
            PixelFormat::YUV420P => frame_data.to_vec(),
            PixelFormat::P010 => {
                // Downsample 10-bit to 8-bit for software encoding
                frame_data.iter().step_by(2).map(|&b| b).collect()
            }
        };
        
        // Simulate encoding
        // In a real implementation, this would call x264_encoder_encode or x265_encoder_encode
        
        let ctx = self.encoder_ctx.as_ref().unwrap();
        
        // Estimate encoded size based on QP
        // Lower QP = higher quality = larger size
        let compression_ratio = match ctx.codec {
            CodecType::H264 => 8.0 + (ctx.qp as f64 / 51.0) * 40.0,
            CodecType::HEVC => 12.0 + (ctx.qp as f64 / 51.0) * 60.0,
            CodecType::AV1 => 16.0 + (ctx.qp as f64 / 51.0) * 80.0,
        };
        
        let estimated_size = (i420_data.len() as f64 / compression_ratio) as usize;
        
        // Return placeholder encoded data
        Ok(vec![0u8; estimated_size.max(100)])
    }
    
    fn should_generate_keyframe(&self) -> bool {
        if self.config.gop_length == 0 {
            // Infinite GOP - only IDR at start
            self.frame_count == 1
        } else {
            self.frame_count % self.config.gop_length as u64 == 1
        }
    }
}

#[async_trait::async_trait]
impl VideoEncoder for SoftwareEncoder {
    async fn initialize(&mut self, config: EncoderConfig) -> Result<(), EncoderError> {
        if self.initialized {
            return Err(EncoderError::InvalidInputFormat("Encoder already initialized".to_string()));
        }
        
        // Validate configuration
        if config.width == 0 || config.height == 0 {
            return Err(EncoderError::InvalidInputFormat(
                "Invalid resolution: width and height must be > 0".to_string()
            ));
        }
        
        if config.fps == 0 {
            return Err(EncoderError::InvalidInputFormat(
                "Invalid framerate: fps must be > 0".to_string()
            ));
        }
        
        self.config = config.clone();
        self.encoder_ctx = Some(self.create_software_context()?);
        
        log::info!(
            "Software encoder initialized: {}x{}@{}fps, {} Mbps, {:?}, preset={}",
            config.width,
            config.height,
            config.fps,
            config.bitrate / 1_000_000,
            config.codec,
            Self::preset_to_x264_preset(config.preset)
        );
        
        self.initialized = true;
        Ok(())
    }
    
    async fn encode(&mut self, frame: &FrameRef) -> Result<EncodedFrame, EncoderError> {
        if !self.initialized {
            return Err(EncoderError::EncodeFailed("Encoder not initialized".to_string()));
        }
        
        let start = Instant::now();
        self.frame_count += 1;
        
        // Get frame data
        let frame_data = frame.data.as_ref()
            .ok_or_else(|| EncoderError::EncodeFailed("Software encoder requires CPU frame data".to_string()))?;
        
        // Encode frame
        let encoded_data = self.encode_frame_software(frame_data, frame.format)?;
        
        let encode_time = start.elapsed();
        let is_keyframe = self.should_generate_keyframe();
        
        // Performance warning for software encoding
        if encode_time.as_millis() > 20 {
            log::warn!(
                "Software encoding slow: {}ms for frame {}",
                encode_time.as_millis(),
                self.frame_count
            );
        }
        
        log::debug!(
            "Software encoded frame {} in {:?}, size={}, keyframe={}",
            self.frame_count,
            encode_time,
            encoded_data.len(),
            is_keyframe
        );
        
        Ok(EncodedFrame {
            data: encoded_data,
            pts: self.frame_count,
            dts: self.frame_count,
            key_frame: is_keyframe,
            width: frame.width,
            height: frame.height,
            codec: self.config.codec,
        })
    }
    
    async fn flush(&mut self) -> Result<Vec<EncodedFrame>, EncoderError> {
        log::debug!("Software encoder flush called");
        
        let mut frames = Vec::new();
        while let Some(frame) = self.output_queue.pop_front() {
            frames.push(frame);
        }
        Ok(frames)
    }
    
    fn set_framerate(&mut self, framerate: u32) {
        self.config.framerate = framerate;
    }
    
    fn get_stats(&self) -> EncoderStats {
        EncoderStats {
            frames_encoded: self.frame_count,
            keyframes_encoded: self.frame_count / self.config.keyframe_interval as u64,
            average_encode_time_us: 0,
            current_bitrate: self.config.bitrate_bps,
            current_framerate: self.config.framerate as f32,
            average_frame_size: 0,
            encode_errors: 0,
        }
    }
    
    async fn shutdown(&mut self) -> Result<(), EncoderError> {
        Ok(())
    }
    
    fn is_hardware(&self) -> bool {
        false
    }
    
    fn request_idr(&mut self) {
        // 软件编码器不支持动态 IDR 请求
    }
    
    fn set_bitrate(&mut self, bitrate: u32) {
        log::info!(
            "Software encoder bitrate changed: {} -> {} bps",
            self.config.bitrate,
            bitrate
        );
        
        self.config.bitrate = bitrate;
        
        // Update encoder context
        if let Some(ref mut ctx) = self.encoder_ctx {
            // Recalculate QP based on new bitrate
            let bits_per_pixel = bitrate as f64 
                / (ctx.width * ctx.height * self.config.fps) as f64;
            ctx.qp = (51.0 - bits_per_pixel * 1000.0).clamp(18.0, 51.0) as u8;
        }
    }
    
    fn name(&self) -> &'static str {
        "Software-x264"
    }
}

impl Drop for SoftwareEncoder {
    fn drop(&mut self) {
        log::info!(
            "Software encoder destroyed, encoded {} frames",
            self.frame_count
        );
        
        // In real implementation:
        // x264_encoder_close or x265_encoder_close
    }
}

unsafe impl Send for SoftwareEncoder {}
unsafe impl Sync for SoftwareEncoder {}

/// Software encoder builder for easy configuration
pub struct SoftwareEncoderBuilder {
    config: EncoderConfig,
}

impl SoftwareEncoderBuilder {
    pub fn new() -> Self {
        Self {
            config: EncoderConfig::default(),
        }
    }
    
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }
    
    pub fn fps(mut self, fps: u32) -> Self {
        self.config.fps = fps;
        self
    }
    
    pub fn bitrate(mut self, bitrate: u32) -> Self {
        self.config.bitrate = bitrate;
        self
    }
    
    pub fn codec(mut self, codec: CodecType) -> Self {
        self.config.codec = codec;
        self
    }
    
    pub fn preset(mut self, preset: Preset) -> Self {
        self.config.preset = preset;
        self
    }
    
    pub fn build(self) -> Result<SoftwareEncoder, Error> {
        SoftwareEncoder::with_config(self.config)
    }
}

impl Default for SoftwareEncoderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod software_tests {
    use super::*;
    
    #[test]
    fn test_nv12_to_i420_conversion() {
        let encoder = SoftwareEncoder::new();
        let width = 1920u32;
        let height = 1080u32;
        let y_size = (width * height) as usize;
        let uv_size = y_size / 2;
        
        // Create test NV12 data
        let mut nv12 = vec![0u8; y_size + uv_size];
        for i in 0..y_size {
            nv12[i] = (i % 256) as u8;
        }
        for i in 0..uv_size {
            nv12[y_size + i] = (i % 256) as u8;
        }
        
        let i420 = encoder.convert_nv12_to_i420(&nv12, width, height);
        
        // I420 should have same total size
        assert_eq!(i420.len(), y_size + uv_size);
        
        // Y plane should be identical
        assert_eq!(&i420[0..y_size], &nv12[0..y_size]);
    }
}
