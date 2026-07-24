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

#[derive(Debug)]
pub struct DmaBufFrame {
    pub fd: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    pub modifier: u64,
    pub format: VideoFormat,
    pub timestamp: u64,
}
