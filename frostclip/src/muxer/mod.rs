use crate::encoder::{AudioInfo, EncodedPacket, VideoInfo};

pub mod mp4;

pub trait Muxer {
    fn write(&mut self, packet: &EncodedPacket) -> Result<(), ()>;

    fn finish(self: Box<Self>) -> Result<(), ()>;
}

pub trait MuxerFactory {
    fn extension(&self) -> &'static str;

    fn create(
        &self,
        path: &str,
        video: &VideoInfo,
        audio: Option<&AudioInfo>,
    ) -> Result<Box<dyn Muxer>, ()>;
}

pub struct Mp4MuxerFactory;

impl MuxerFactory for Mp4MuxerFactory {
    fn extension(&self) -> &'static str {
        "mp4"
    }

    fn create(
        &self,
        path: &str,
        video: &VideoInfo,
        audio: Option<&AudioInfo>,
    ) -> Result<Box<dyn Muxer>, ()> {
        Ok(Box::new(mp4::Mp4Muxer::create(path, video, audio)?))
    }
}
