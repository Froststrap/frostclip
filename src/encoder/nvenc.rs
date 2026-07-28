use anyhow::{Result, anyhow};

use crate::{
    encoder::{EncodedPacket, Encoder, VideoInfo},
    frame::VideoFrame,
};

pub struct NvencEncoder {}

impl NvencEncoder {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

impl Encoder for NvencEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>> {
        match frame {
            VideoFrame::LinuxDmaBuf(dmabuf) => {
                // TODO: actually implement nvenc support

                todo!("NVENC support not yet implemented")
            }

            _ => Err(anyhow!("NVENC only supports DMA-BUF frames")),
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }

    fn video_info(&self) -> Option<VideoInfo> {
        None
    }
}
