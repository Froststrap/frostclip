use ffmpeg_sys_next::*;
use std::ffi::CString;

pub struct DrmHardware {
    device_ctx: *mut AVBufferRef,
    frames_ctx: *mut AVBufferRef,
}

unsafe impl Send for DrmHardware {}

impl DrmHardware {
    pub fn new(path: &str, width: u32, height: u32) -> Result<Self, String> {
        let path = CString::new(path).unwrap();
        let mut device_ctx: *mut AVBufferRef = std::ptr::null_mut();

        let ret = unsafe {
            av_hwdevice_ctx_create(
                &mut device_ctx,
                AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
                path.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        };

        if ret < 0 {
            return Err(format!("DRM device creation failed {}", ret));
        }

        let mut frames_ctx = unsafe { av_hwframe_ctx_alloc(device_ctx) };

        if frames_ctx.is_null() {
            unsafe {
                av_buffer_unref(&mut device_ctx);
            }
            return Err("DRM frame context allocation failed".into());
        }

        let frames = unsafe { &mut *((*frames_ctx).data as *mut AVHWFramesContext) };

        frames.format = AVPixelFormat::AV_PIX_FMT_DRM_PRIME;
        frames.sw_format = AVPixelFormat::AV_PIX_FMT_BGR0;
        frames.width = width as i32;
        frames.height = height as i32;

        let ret = unsafe { av_hwframe_ctx_init(frames_ctx) };

        if ret < 0 {
            unsafe {
                av_buffer_unref(&mut frames_ctx);
                av_buffer_unref(&mut device_ctx);
            }
            return Err(format!("DRM frame context init failed {}", ret));
        }

        Ok(Self {
            device_ctx,
            frames_ctx,
        })
    }

    pub fn frames_ctx(&self) -> *mut AVBufferRef {
        self.frames_ctx
    }
}

impl Drop for DrmHardware {
    fn drop(&mut self) {
        unsafe {
            av_buffer_unref(&mut self.frames_ctx);
            av_buffer_unref(&mut self.device_ctx);
        }
    }
}
