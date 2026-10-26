mod dmabuf;
#[cfg(target_os = "macos")]
pub mod iosurface;

pub use dmabuf::DmaBufFrame;
#[cfg(target_os = "macos")]
pub use iosurface::SendPixelBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    Unknown,
    Bgra,
    Rgba,
    Nv12,
    Yuyv,
}

#[derive(Debug)]
pub enum VideoFrame {
    LinuxDmaBuf(DmaBufFrame),

    #[cfg(target_os = "macos")]
    MacIoSurface(iosurface::IoSurfaceFrame),

    Cpu {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        format: VideoFormat,
        timestamp: u64,
    },
}
