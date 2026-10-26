use super::{VideoFormat, VideoFrame};
use objc2_core_foundation::CFRetained;
use objc2_core_video::*;
use std::fmt;

pub struct SendPixelBuffer(pub CFRetained<CVPixelBuffer>);
unsafe impl Send for SendPixelBuffer {}
unsafe impl Sync for SendPixelBuffer {}

impl fmt::Debug for SendPixelBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SendPixelBuffer(..)")
    }
}

#[derive(Debug)]
pub struct IoSurfaceFrame {
    pub buffer: SendPixelBuffer,
    pub width: u32,
    pub height: u32,
    pub timestamp: u64,
}

impl IoSurfaceFrame {
    pub fn into_cpu(self) -> Result<VideoFrame, String> {
        let pixels = &self.buffer.0;
        let width = self.width as usize;
        let height = self.height as usize;
        let row_bytes = width * 4;

        unsafe {
            if CVPixelBufferLockBaseAddress(pixels, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return Err("failed to lock pixel buffer".into());
            }

            let base = CVPixelBufferGetBaseAddress(pixels) as *const u8;
            let stride = CVPixelBufferGetBytesPerRow(pixels);
            let mut out = vec![0u8; row_bytes * height];

            for y in 0..height {
                let src = std::slice::from_raw_parts(base.add(y * stride), row_bytes);
                let dst = &mut out[y * row_bytes..(y + 1) * row_bytes];
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }

            CVPixelBufferUnlockBaseAddress(pixels, CVPixelBufferLockFlags::ReadOnly);

            Ok(VideoFrame::Cpu {
                data: out,
                width: self.width,
                height: self.height,
                stride: row_bytes as u32,
                format: VideoFormat::Rgba,
                timestamp: self.timestamp,
            })
        }
    }
}
