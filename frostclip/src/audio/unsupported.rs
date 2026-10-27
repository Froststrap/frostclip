use super::{AudioBackend, AudioError, AudioFrame, AudioSource, Result};
use std::sync::mpsc::Receiver;

pub struct PlatformAudio;

impl PlatformAudio {
    pub fn new(_source: AudioSource) -> Result<Self> {
        Err(AudioError::Unsupported)
    }
}

impl AudioBackend for PlatformAudio {
    fn start(&mut self) -> Result<Receiver<AudioFrame>> {
        Err(AudioError::Unsupported)
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
