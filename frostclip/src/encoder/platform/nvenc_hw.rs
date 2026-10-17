use ffmpeg_sys_next::*;

pub struct NvencHardware {
    device_ctx: *mut AVBufferRef,
    frames_ctx: *mut AVBufferRef,
}

unsafe impl Send for NvencHardware {}

impl NvencHardware {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let mut device_ctx: *mut AVBufferRef = std::ptr::null_mut();

        // NVENC uses CUDA. Passing null for device selects the default GPU.
        let ret = unsafe {
            av_hwdevice_ctx_create(
                &mut device_ctx,
                AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            )
        };

        if ret < 0 {
            return Err(format!("CUDA device creation failed {}", ret));
        }

        let frames_ctx = unsafe { av_hwframe_ctx_alloc(device_ctx) };

        if frames_ctx.is_null() {
            unsafe {
                av_buffer_unref(&mut device_ctx);
            }

            return Err("CUDA frame context allocation failed".into());
        }

        let frames = unsafe { &mut *((*frames_ctx).data as *mut AVHWFramesContext) };

        frames.format = AVPixelFormat::AV_PIX_FMT_CUDA;

        frames.sw_format = AVPixelFormat::AV_PIX_FMT_NV12;

        frames.width = width as i32;
        frames.height = height as i32;

        frames.initial_pool_size = 20;

        let ret = unsafe { av_hwframe_ctx_init(frames_ctx) };

        if ret < 0 {
            unsafe {
                av_buffer_unref(&mut frames_ctx.cast());
                av_buffer_unref(&mut device_ctx);
            }

            return Err(format!("CUDA frame context init failed {}", ret));
        }

        Ok(Self {
            device_ctx,
            frames_ctx,
        })
    }

    pub fn device_ctx(&self) -> *mut AVBufferRef {
        self.device_ctx
    }

    pub fn frames_ctx(&self) -> *mut AVBufferRef {
        self.frames_ctx
    }
}

impl Drop for NvencHardware {
    fn drop(&mut self) {
        unsafe {
            av_buffer_unref(&mut self.frames_ctx);

            av_buffer_unref(&mut self.device_ctx);
        }
    }
}
