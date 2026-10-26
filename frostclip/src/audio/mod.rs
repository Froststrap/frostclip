use anyhow::Result;
use std::sync::mpsc::Receiver;

mod frame;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unsupported;

pub use frame::{AudioFormat, AudioFrame};

#[cfg(target_os = "linux")]
use linux::PlatformAudio;
#[cfg(not(target_os = "linux"))]
use unsupported::PlatformAudio;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSource {
    Desktop,
    Microphone,
    Both,
}

pub trait AudioBackend: Send {
    fn start(&mut self) -> Result<Receiver<AudioFrame>>;
    fn stop(&mut self) -> Result<()>;
}

pub struct AudioCapture {
    backend: Box<dyn AudioBackend>,
}

impl AudioCapture {
    pub fn new(source: AudioSource) -> Result<Self> {
        Ok(Self {
            backend: Box::new(PlatformAudio::new(source)?),
        })
    }

    pub fn start(&mut self) -> Result<Receiver<AudioFrame>> {
        self.backend.start()
    }

    pub fn stop(&mut self) -> Result<()> {
        self.backend.stop()
    }
}
