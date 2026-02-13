//! 零拷贝屏幕捕获模块的集成测试
//!
//! 这些测试验证所有捕获后端的正确性和性能。

#[cfg(test)]
mod tests {
    use crate::capture::*;

    /// 测试自动检测功能
    #[test]
    fn test_detect_best_capture() {
        // 这个测试可能会失败（如果没有可用的捕获后端）
        // 但至少应该能运行而不 panic
        match detect_best_capture() {
            Ok(capture) => {
                let name = capture.name();
                assert!(
                    name == "NvFBC" || name == "KMS/DRM" || name == "X11",
                    "未知的捕获后端: {}",
                    name
                );
                println!("自动检测到的捕获后端: {}", name);
            }
            Err(e) => {
                println!("没有可用的捕获后端: {}", e);
                // 在某些环境中这是预期的
            }
        }
    }

    /// 测试列出可用后端
    #[test]
    fn test_list_available_backends() {
        let backends = list_available_backends();
        println!("可用捕获后端: {:?}", backends);
        // 即使没有后端，也应该返回空列表
        assert!(backends.len() <= 3);
    }

    /// 测试 NvFBC 捕获
    #[test]
    #[cfg(feature = "nvfbc_tests")]
    fn test_nvfbcs_capture() {
        if !is_nvfbcs_available() {
            println!("跳过测试：NvFBC 不可用");
            return;
        }

        let mut capture = nvfbcs::NvFBCCapture::new().unwrap();
        capture.init().unwrap();

        let frame = capture.capture().unwrap();
        
        // 验证帧属性
        assert!(frame.is_valid(), "帧应该是有效的");
        assert!(
            frame.gpu_ptr.is_some() || frame.dmabuf_fd.is_some(),
            "帧应该有 GPU 指针或 DMA-BUF fd"
        );
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert!(frame.data_size > 0);

        capture.cleanup();
    }

    /// 测试 KMS 捕获
    #[test]
    #[cfg(feature = "kms_tests")]
    fn test_kms_capture() {
        if !is_kms_available() {
            println!("跳过测试：KMS 不可用");
            return;
        }

        let mut capture = kms::KmsCapture::new().unwrap();
        capture.init().unwrap();

        let frame = capture.capture().unwrap();
        
        // 验证帧属性
        assert!(frame.is_valid(), "帧应该是有效的");
        assert!(
            frame.dmabuf_fd.is_some(),
            "KMS 帧应该有 DMA-BUF fd"
        );
        assert!(frame.width > 0);
        assert!(frame.height > 0);
        assert!(frame.data_size > 0);

        capture.cleanup();
    }

    /// 测试 X11 捕获
    #[test]
    #[cfg(feature = "x11_tests")]
    fn test_x11_capture() {
        if !is_x11_available() {
            println!("跳过测试：X11 不可用");
            return;
        }

        let mut capture = x11::X11Capture::new().unwrap();
        capture.init().unwrap();

        let frame = capture.capture().unwrap();
        
        // X11 捕获不是零拷贝，但应该返回有效帧
        assert!(frame.width > 0);
        assert!(frame.height > 0);
        assert!(frame.data_size > 0);

        capture.cleanup();
    }

    /// 测试所有后端的一致性
    #[test]
    fn test_backend_consistency() {
        // 测试所有后端的通用行为
        let mut backends: Vec<Box<dyn CaptureBackend>> = Vec::new();

        // 尝试创建所有可用的后端
        if is_nvfbcs_available() {
            if let Ok(capture) = nvfbcs::NvFBCCapture::new() {
                backends.push(Box::new(capture));
            }
        }

        if is_kms_available() {
            if let Ok(capture) = kms::KmsCapture::new() {
                backends.push(Box::new(capture));
            }
        }

        if is_x11_available() {
            if let Ok(capture) = x11::X11Capture::new() {
                backends.push(Box::new(capture));
            }
        }

        // 验证每个后端的一致性
        for backend in &mut backends {
            let name = backend.name();
            println!("测试后端: {}", name);

            // 检查名称
            assert!(
                name == "NvFBC" || name == "KMS/DRM" || name == "X11",
                "未知的后端名称: {}",
                name
            );

            // 检查分辨率
            let (width, height) = backend.resolution();
            assert!(width > 0, "{} 宽度应该大于 0", name);
            assert!(height > 0, "{} 高度应该大于 0", name);

            // 检查零拷贝标志
            let is_zc = backend.is_zero_copy();
            if name == "X11" {
                assert!(!is_zc, "X11 不应该支持零拷贝");
            } else {
                assert!(is_zc, "{} 应该支持零拷贝", name);
            }
        }
    }

    /// 测试分辨率设置
    #[test]
    fn test_resolution_setting() {
        // 测试 X11（通常总是可用）
        if is_x11_available() {
            if let Ok(mut capture) = x11::X11Capture::new() {
                let result = capture.set_resolution(1280, 720);
                assert!(result.is_ok());
                assert_eq!(capture.resolution(), (1280, 720));

                let result = capture.set_resolution(1920, 1080);
                assert!(result.is_ok());
                assert_eq!(capture.resolution(), (1920, 1080));
            }
        }
    }

    /// 测试帧引用生命周期
    #[test]
    fn test_frame_ref_lifecycle() {
        // 创建帧
        let frame = FrameRef::new(1920, 1080, PixelFormat::BGRA);
        
        // 验证初始状态
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert!(frame.gpu_ptr.is_none());
        assert!(frame.dmabuf_fd.is_none());

        // 创建带 GPU 指针的帧
        let frame_with_gpu = FrameRef::with_gpu_ptr(1920, 1080, PixelFormat::RGBA, 0x12345678);
        assert_eq!(frame_with_gpu.gpu_ptr, Some(0x12345678));
        assert!(frame_with_gpu.is_valid());

        // 帧在这里被 drop，应该正常释放资源
    }

    /// 测试错误处理
    #[test]
    fn test_error_handling() {
        // 测试各种错误类型
        let init_error = CaptureError::InitFailed("测试初始化失败".to_string());
        assert!(init_error.to_string().contains("初始化失败"));

        let capture_error = CaptureError::CaptureFailed("测试捕获失败".to_string());
        assert!(capture_error.to_string().contains("捕获失败"));

        let cuda_error = CaptureError::CudaError("测试 CUDA 错误".to_string());
        assert!(cuda_error.to_string().contains("CUDA"));

        let drm_error = CaptureError::DrmError("测试 DRM 错误".to_string());
        assert!(drm_error.to_string().contains("DRM"));
    }

    /// 性能基准测试（可选）
    #[test]
    #[ignore] // 默认忽略，需要手动运行
    fn benchmark_capture_performance() {
        use std::time::Instant;

        let mut capture = match detect_best_capture() {
            Ok(c) => c,
            Err(_) => {
                println!("跳过基准测试：没有可用的捕获后端");
                return;
            }
        };

        // 预热
        for _ in 0..10 {
            let _ = capture.capture();
        }

        // 正式测试
        let iterations = 100;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = capture.capture();
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed.as_secs_f64() / iterations as f64;
        let fps = 1.0 / avg_time;

        println!("捕获性能基准:");
        println!("  总时间: {:?}", elapsed);
        println!("  平均每次: {:.3} ms", avg_time * 1000.0);
        println!("  估计 FPS: {:.1}", fps);

        capture.cleanup();
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::capture::{PixelFormat, FrameRef};

    /// 测试像素格式属性
    #[test]
    fn test_pixel_format_properties() {
        // 测试所有格式都有有效的名称
        for format in [
            PixelFormat::BGRA,
            PixelFormat::RGBA,
            PixelFormat::NV12,
            PixelFormat::YUV420,
        ] {
            let name = format.name();
            assert!(!name.is_empty());
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric()));
        }
    }

    /// 测试不同分辨率下的帧大小计算
    #[test]
    fn test_frame_size_calculations() {
        let test_cases = [
            ((1920, 1080), PixelFormat::BGRA, 1920 * 1080 * 4),
            ((1920, 1080), PixelFormat::RGBA, 1920 * 1080 * 4),
            ((1920, 1080), PixelFormat::NV12, 1920 * 1080 * 3 / 2),
            ((1280, 720), PixelFormat::BGRA, 1280 * 720 * 4),
            ((2560, 1440), PixelFormat::RGBA, 2560 * 1440 * 4),
            ((3840, 2160), PixelFormat::NV12, 3840 * 2160 * 3 / 2),
        ];

        for ((w, h), format, expected) in &test_cases {
            let frame = FrameRef::new(*w, *h, *format);
            assert_eq!(
                frame.data_size,
                *expected as usize,
                "分辨率 {}x{} 格式 {:?} 大小不匹配",
                w,
                h,
                format
            );
        }
    }

    /// 测试边界情况
    #[test]
    fn test_edge_cases() {
        // 最小分辨率
        let frame = FrameRef::new(1, 1, PixelFormat::BGRA);
        assert_eq!(frame.data_size, 4);

        // 奇数分辨率 (YUV 格式需要处理)
        let frame = FrameRef::new(1921, 1081, PixelFormat::NV12);
        let expected = (1921 * 1081 * 3 / 2) as usize;
        assert_eq!(frame.data_size, expected);
    }
}
