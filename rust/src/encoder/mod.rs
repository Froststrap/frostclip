use crate::frame::VideoFrame;

pub mod factory;
pub mod nvenc;
pub mod software;
pub mod vaapi;
pub mod vaapi_hw;

#[derive(Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,

    pub time_base_num: i32,
    pub time_base_den: i32,

    pub extradata: Vec<u8>,
    pub codec: ffmpeg_next::codec::Id,
    pub format: ffmpeg_next::format::Pixel,
}

pub trait Encoder: Send {
    fn submit(&mut self, frame: VideoFrame) -> anyhow::Result<Vec<EncodedPacket>>;

    fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>>;

    fn video_info(&self) -> Option<VideoInfo>;
}
