pub struct AudioCapture {
    mode: String,
}

impl AudioCapture {
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.to_string(),
        }
    }

    pub fn capture(&mut self) -> Result<()> {
        // TODO: implement PulseAudio/PipeWire capture
        // Use a library for audio capture (most likely cpal)
        Ok(())
    }
}
