mod dmabuf;

pub use dmabuf::DmaBufFrame;

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

    Cpu {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        format: VideoFormat,
        timestamp: u64,
    },
}
