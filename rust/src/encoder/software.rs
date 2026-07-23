use anyhow::Result;

use crate::{
    encoder::{EncodedPacket, Encoder},
    frame::VideoFrame,
};

pub struct SoftwareEncoder {}

impl SoftwareEncoder {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

impl Encoder for SoftwareEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>> {
        match frame {
            VideoFrame::Cpu { data, .. } => {
                /*
                    libx264 here
                */

                todo!("x264 encode")
            }

            _ => {
                todo!("download dma-buf to cpu")
            }
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}
