//! Encoder Module Unit Tests

use super::*;

// ============================================================================
// Basic Type Tests
// ============================================================================

#[test]
fn test_codec_type_display() {
    assert_eq!(format!("{}", CodecType::H264), "H264");
    assert_eq!(format!("{}", CodecType::HEVC), "HEVC");
    assert_eq!(format!("{}", CodecType::AV1), "AV1");
}

#[test]
fn test_preset_to_nvenc() {
    assert_eq!(Preset::P1.to_nvenc_preset(), 1);
    assert_eq!(Preset::P4.to_nvenc_preset(), 4);
    assert_eq!(Preset::P7.to_nvenc_preset(), 7);
}

#[test]
fn test_encoder_config_default() {
    let config = EncoderConfig::default();
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.fps, 60);
    assert_eq!(config.bitrate, 20_000_000);
    assert_eq!(config.codec, CodecType::H264);
    assert_eq!(config.bframes, 0);
    assert_eq!(config.gop_length, 0);
}

// ============================================================================
// FrameRef Tests
// ============================================================================

#[test]
fn test_frame_ref_from_data() {
    let data = vec![0u8; 1920 * 1080 * 4];
    let frame = FrameRef::from_data(data.clone(), 1920, 1080, PixelFormat::RGBA);
    
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
    assert_eq!(frame.format, PixelFormat::RGBA);
    assert!(frame.data.is_some());
    assert!(frame.gpu_ptr.is_none());
    assert!(frame.dmabuf_fd.is_none());
}

#[test]
fn test_frame_ref_from_dmabuf() {
    let frame = FrameRef::from_dmabuf(42, 1920, 1080, PixelFormat::NV12);
    
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
    assert_eq!(frame.format, PixelFormat::NV12);
    assert!(frame.data.is_none());
    assert!(frame.dmabuf_fd.is_some());
    assert_eq!(frame.dmabuf_fd.unwrap(), 42);
}

// ============================================================================
// Software Encoder Tests
// ============================================================================

#[test]
fn test_software_encoder_init() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 1280,
        height: 720,
        fps: 30,
        bitrate: 5_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 30,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    assert!(encoder.init(&config).is_ok());
    assert_eq!(encoder.name(), "Software-x264");
}

#[test]
fn test_software_encoder_invalid_resolution() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 0,
        height: 1080,
        fps: 60,
        bitrate: 20_000_000,
        codec: CodecType::H264,
        preset: Preset::default(),
        tuning: Tuning::default(),
        gop_length: 0,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    assert!(encoder.init(&config).is_err());
}

#[test]
fn test_software_encoder_encode() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 640,
        height: 480,
        fps: 30,
        bitrate: 2_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 30,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    encoder.init(&config).unwrap();
    
    // Create test frame
    let frame_data = vec![128u8; 640 * 480 * 4]; // RGBA
    let frame = FrameRef::from_data(frame_data, 640, 480, PixelFormat::RGBA);
    
    let result = encoder.encode(&frame);
    assert!(result.is_ok());
    
    let encoded = result.unwrap();
    assert_eq!(encoded.width, 640);
    assert_eq!(encoded.height, 480);
    assert_eq!(encoded.codec, CodecType::H264);
    assert!(encoded.key_frame); // First frame should be keyframe
}

#[test]
fn test_software_encoder_keyframe_pattern() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 640,
        height: 480,
        fps: 30,
        bitrate: 2_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 5, // Short GOP for testing
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    encoder.init(&config).unwrap();
    
    let frame_data = vec![128u8; 640 * 480 * 4];
    
    // Encode multiple frames and check keyframe pattern
    for i in 0..10 {
        let frame = FrameRef::from_data(frame_data.clone(), 640, 480, PixelFormat::RGBA);
        let encoded = encoder.encode(&frame).unwrap();
        
        if i % 5 == 0 {
            assert!(encoded.key_frame, "Frame {} should be keyframe", i);
        }
    }
}

#[test]
fn test_software_encoder_set_bitrate() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig::default();
    encoder.init(&config).unwrap();
    
    encoder.set_bitrate(10_000_000);
    assert_eq!(encoder.config().bitrate, 10_000_000);
}

#[test]
fn test_software_encoder_flush() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig::default();
    encoder.init(&config).unwrap();
    
    let result = encoder.flush();
    assert!(result.is_ok());
}

// ============================================================================
// NVENC Encoder Tests
// ============================================================================

#[test]
fn test_nvenc_encoder_creation() {
    let encoder = nvenc_lowlatency::LowLatencyNvenc::new();
    // Should succeed even without hardware (placeholder implementation)
    assert!(encoder.is_ok());
}

#[test]
fn test_nvenc_encoder_init() {
    let mut encoder = nvenc_lowlatency::LowLatencyNvenc::new().unwrap();
    let config = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 60,
        bitrate: 20_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 0,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    assert!(encoder.init(&config).is_ok());
    assert_eq!(encoder.name(), "NVENC-LowLatency");
}

#[test]
fn test_nvenc_double_init_fails() {
    let mut encoder = nvenc_lowlatency::LowLatencyNvenc::new().unwrap();
    let config = EncoderConfig::default();
    
    encoder.init(&config).unwrap();
    assert!(encoder.init(&config).is_err());
}

#[test]
fn test_nvenc_encode_without_init_fails() {
    let mut encoder = nvenc_lowlatency::LowLatencyNvenc::new().unwrap();
    let frame_data = vec![0u8; 1920 * 1080 * 4];
    let frame = FrameRef::from_data(frame_data, 1920, 1080, PixelFormat::RGBA);
    
    assert!(encoder.encode(&frame).is_err());
}

// ============================================================================
// AMF Encoder Tests
// ============================================================================

#[test]
fn test_amf_encoder_creation() {
    // AMF might not be available, so creation may fail on non-AMD systems
    let _encoder = amf::AmfEncoder::new();
    // Just verify it compiles and runs
}

#[test]
fn test_amf_encoder_name() {
    if let Ok(encoder) = amf::AmfEncoder::new() {
        assert_eq!(encoder.name(), "AMF");
    }
}

// ============================================================================
// VAAPI Encoder Tests
// ============================================================================

#[test]
fn test_vaapi_enumerate_devices() {
    let devices = vaapi::enumerate_vaapi_devices();
    // Should return a list (possibly empty if no devices)
    assert!(devices.is_empty() || devices.iter().all(|d| d.starts_with("/dev/dri/")));
}

#[test]
fn test_vaapi_encoder_with_invalid_device() {
    let result = vaapi::VaapiEncoder::with_device("/dev/nonexistent");
    assert!(result.is_err());
}

// ============================================================================
// Encoder Detection Tests
// ============================================================================

#[test]
fn test_detect_gpu() {
    // This test may or may not find a GPU depending on the system
    let gpu = detect_gpu();
    
    if let Some(info) = gpu {
        assert!(!info.name.is_empty());
        // At least one encoding capability should be true
        assert!(
            info.has_nvenc || info.has_amf || info.has_vaapi || info.has_quicksync,
            "GPU should have at least one encoding capability"
        );
    }
}

#[test]
fn test_detect_best_encoder() {
    // Should always return an encoder (software fallback)
    let encoder = detect_best_encoder();
    let name = encoder.name();
    assert!(
        name == "NVENC-LowLatency" || 
        name == "AMF" || 
        name == "VAAPI" || 
        name == "Software-x264"
    );
}

#[test]
fn test_create_encoder_backends() {
    // Test creating specific backends
    let nvenc = create_encoder(EncoderBackend::NVENC);
    let amf = create_encoder(EncoderBackend::AMF);
    let vaapi = create_encoder(EncoderBackend::VAAPI);
    let software = create_encoder(EncoderBackend::Software);
    
    // Software should always succeed
    assert!(software.is_ok());
    assert_eq!(software.unwrap().name(), "Software-x264");
    
    // Hardware encoders may fail if not available
    // Just verify they return a Result
    let _ = nvenc;
    let _ = amf;
    let _ = vaapi;
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[test]
fn test_thread_safe_encoder() {
    let encoder = create_encoder(EncoderBackend::Software).unwrap();
    let thread_safe = ThreadSafeEncoder::new(encoder);
    
    // Test that ThreadSafeEncoder implements Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ThreadSafeEncoder>();
    
    // Test encoding
    let frame_data = vec![0u8; 640 * 480 * 4];
    let frame = FrameRef::from_data(frame_data, 640, 480, PixelFormat::RGBA);
    
    let result = thread_safe.encode(&frame);
    assert!(result.is_ok());
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_encode_pipeline() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 1280,
        height: 720,
        fps: 60,
        bitrate: 8_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 60,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    
    encoder.init(&config).unwrap();
    
    // Simulate encoding a sequence of frames
    let mut total_encoded_size = 0usize;
    let mut keyframe_count = 0;
    
    for i in 0..120 {
        let frame_data = vec![(i % 256) as u8; 1280 * 720 * 4];
        let frame = FrameRef::from_data(frame_data, 1280, 720, PixelFormat::RGBA);
        
        let encoded = encoder.encode(&frame).unwrap();
        total_encoded_size += encoded.data.len();
        
        if encoded.key_frame {
            keyframe_count += 1;
        }
        
        // Verify frame properties
        assert_eq!(encoded.width, 1280);
        assert_eq!(encoded.height, 720);
        assert_eq!(encoded.codec, CodecType::H264);
    }
    
    // Should have approximately 2 keyframes (one every 60 frames)
    assert!(keyframe_count >= 1 && keyframe_count <= 3);
    
    // Verify compression occurred
    let raw_size = 1280 * 720 * 4 * 120;
    assert!(total_encoded_size < raw_size / 4, "Compression ratio too low");
    
    log::info!(
        "Encoded 120 frames: {} keyframes, total size: {} bytes",
        keyframe_count,
        total_encoded_size
    );
}

#[test]
fn test_dynamic_bitrate_adjustment() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig::default();
    encoder.init(&config).unwrap();
    
    // Encode at default bitrate
    let frame_data = vec![128u8; 1920 * 1080 * 4];
    let frame1 = FrameRef::from_data(frame_data.clone(), 1920, 1080, PixelFormat::RGBA);
    let encoded1 = encoder.encode(&frame1).unwrap();
    
    // Reduce bitrate
    encoder.set_bitrate(5_000_000);
    
    // Encode at lower bitrate
    let frame2 = FrameRef::from_data(frame_data, 1920, 1080, PixelFormat::RGBA);
    let encoded2 = encoder.encode(&frame2).unwrap();
    
    // Lower bitrate should generally produce smaller frames
    // (though this depends on content, so we just verify it doesn't panic)
    log::info!(
        "Bitrate adjustment: {} bytes -> {} bytes",
        encoded1.data.len(),
        encoded2.data.len()
    );
}

#[test]
fn test_all_pixel_formats() {
    let mut encoder = software::SoftwareEncoder::new();
    let config = EncoderConfig {
        width: 640,
        height: 480,
        fps: 30,
        bitrate: 2_000_000,
        codec: CodecType::H264,
        preset: Preset::P1,
        tuning: Tuning::LowLatency,
        gop_length: 30,
        bframes: 0,
        rc_mode: RateControlMode::CBR,
    };
    encoder.init(&config).unwrap();
    
    let formats = vec![
        (PixelFormat::NV12, 640 * 480 * 3 / 2),
        (PixelFormat::YUV420P, 640 * 480 * 3 / 2),
        (PixelFormat::RGBA, 640 * 480 * 4),
        (PixelFormat::BGRA, 640 * 480 * 4),
    ];
    
    for (format, size) in formats {
        let frame_data = vec![128u8; size];
        let frame = FrameRef::from_data(frame_data, 640, 480, format);
        let result = encoder.encode(&frame);
        assert!(result.is_ok(), "Failed to encode {:?}", format);
    }
}

#[test]
fn test_encoded_frame_properties() {
    let mut encoder = software::SoftwareEncoder::new();
    encoder.init(&EncoderConfig::default()).unwrap();
    
    let frame_data = vec![0u8; 1920 * 1080 * 4];
    let frame = FrameRef::from_data(frame_data, 1920, 1080, PixelFormat::RGBA);
    
    let encoded = encoder.encode(&frame).unwrap();
    
    // Verify all EncodedFrame fields
    assert!(!encoded.data.is_empty());
    assert_eq!(encoded.width, 1920);
    assert_eq!(encoded.height, 1080);
    assert!(encoded.pts > 0);
    assert!(encoded.dts > 0);
}
