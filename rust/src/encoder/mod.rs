use crate::frame::VideoFrame;

pub mod nvenc;
pub mod software;
pub mod vaapi;

#[derive(Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub is_keyframe: bool,
}

pub trait Encoder: Send {
    fn submit(&mut self, frame: VideoFrame) -> anyhow::Result<Vec<EncodedPacket>>;

    fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>>;
}
