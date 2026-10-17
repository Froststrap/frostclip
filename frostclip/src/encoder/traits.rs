use crate::frame::VideoFrame;

use super::{EncodedPacket, VideoInfo};

pub trait Encoder: Send {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()>;

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()>;

    fn video_info(&self) -> Option<VideoInfo>;

    fn accepts_dmabuf(&self) -> bool {
        false
    }
}
