use anyhow::{Result, anyhow};

use crate::{
    encoder::{EncodedPacket, Encoder},
    frame::VideoFrame,
};

pub struct VaapiEncoder {
    frame_count: u64,
}

impl VaapiEncoder {
    pub fn new() -> Result<Self> {
        Ok(Self { frame_count: 0 })
    }
}

impl Encoder for VaapiEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>> {
        match frame {
            VideoFrame::LinuxDmaBuf(_dmabuf) => {
                self.frame_count += 1;

                println!("VAAPI received DMA-BUF frame {}", self.frame_count);

                /*
                    TEMPORARY MOCK ENCODE

                    Replace this later with:

                    vaCreateSurfaces()
                    vaImportSurfaceHandle()
                    vaBeginPicture()
                    vaRenderPicture()
                    vaEndPicture()
                    vaSyncSurface()
                */

                let packet = EncodedPacket {
                    data: vec![0, 0, 0, 1, 0x65],
                    timestamp: self.frame_count,
                    is_keyframe: self.frame_count % 60 == 0,
                };

                Ok(vec![packet])
            }

            _ => Err(anyhow!("VAAPI requires dma-buf")),
        }
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}
