use anyhow::Result;

use crate::frame::VideoFrame;

pub mod linux;

#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub width: u32,
    pub height: u32,
}

pub trait CaptureBackend {
    fn start(&mut self, callback: Box<dyn Fn(VideoFrame) + Send + Sync>) -> Result<()>;

    fn stop(&mut self) -> Result<()>;

    fn info(&self) -> CaptureInfo;
}

pub struct Capture {
    backend: Box<dyn CaptureBackend>,
}

impl Capture {
    pub fn new() -> Result<Self> {
        Ok(Self {
            backend: Box::new(linux::LinuxCapture::new()?),
        })
    }

    pub fn start(&mut self, callback: Box<dyn Fn(VideoFrame) + Send + Sync>) -> Result<()> {
        self.backend.start(callback)
    }

    pub fn stop(&mut self) -> Result<()> {
        self.backend.stop()
    }

    pub fn info(&self) -> CaptureInfo {
        self.backend.info()
    }
}
