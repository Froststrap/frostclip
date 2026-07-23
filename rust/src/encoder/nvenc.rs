use anyhow::{Result, anyhow};

use crate::{
    encoder::{EncodedPacket, Encoder},
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
                /*
                    - Create CUDA context
                    - Import dma-buf:
                    - cuImportExternalMemory()
                    - Map CUDA array
                    - Register NVENC resource
                    - nvEncEncodePicture()
                */

                todo!("NVENC dma-buf path")
            }

            _ => Err(anyhow!("NVENC only supports DMA-BUF frames")),
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}
