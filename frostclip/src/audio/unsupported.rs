use super::{AudioBackend, AudioFrame, AudioSource};
use anyhow::{Result, bail};
use std::sync::mpsc::Receiver;

pub struct PlatformAudio;

impl PlatformAudio {
    pub fn new(_source: AudioSource) -> Result<Self> {
        bail!("audio capture not supported on this platform")
    }
}

impl AudioBackend for PlatformAudio {
    fn start(&mut self) -> Result<Receiver<AudioFrame>> {
        bail!("audio capture not supported on this platform")
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
