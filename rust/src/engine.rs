use anyhow::Result;
use std::sync::{Arc, Mutex};

use log::{debug, error, info};

use crate::buffer::ReplayBuffer;
use crate::capture::Capture;
use crate::config::Config;
use crate::encoder::{
    Encoder,
    factory::{EncoderConfig, create_encoder},
};
use crate::frame::VideoFrame;

pub struct CaptureEngine {
    capture: Capture,
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    buffer: Arc<Mutex<ReplayBuffer>>,
    config: Config,
    running: bool,
}

impl CaptureEngine {
    pub fn new(config: Config) -> Result<Self> {
        info!("ENGINE: creating capture engine");

        config.ensure_output_dir()?;

        info!("ENGINE: output directory {:?}", config.output_dir);

        let capture = Capture::new()?;
        let capture_info = capture.info();

        info!("ENGINE: capture backend created");
        info!(
            "ENGINE: capture resolution {}x{}",
            capture_info.width, capture_info.height
        );

        let encoder = create_encoder(EncoderConfig {
            input_width: capture_info.width,
            input_height: capture_info.height,

            output_width: config.width,
            output_height: config.height,
        })?;
        info!("ENGINE: encoder created");

        let buffer = ReplayBuffer::new(
            config.framerate as usize,
            30,
            config.width as usize,
            config.height as usize,
        );

        info!(
            "ENGINE: replay buffer created max_frames={}",
            config.framerate * 30
        );

        Ok(Self {
            capture,
            encoder: Arc::new(Mutex::new(encoder)),
            buffer: Arc::new(Mutex::new(buffer)),
            config,
            running: false,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        info!("ENGINE: start requested");

        let encoder = Arc::clone(&self.encoder);

        let buffer = Arc::clone(&self.buffer);

        self.capture.start(Box::new(move |frame| {
            debug!("ENGINE: frame callback received");

            let packets = match encoder.lock().unwrap().submit(frame) {
                Ok(packets) => {
                    debug!("ENGINE: encoder produced {} packets", packets.len());

                    packets
                }

                Err(err) => {
                    error!("ENGINE: encoder submit failed {:?}", err);

                    return;
                }
            };

            let mut buffer = buffer.lock().unwrap();

            for packet in packets {
                debug!(
                    "ENGINE: pushing packet timestamp={} keyframe={}",
                    packet.timestamp, packet.is_keyframe
                );

                buffer.push(packet);
            }

            debug!("ENGINE: replay buffer size={}", buffer.len());
        }))?;

        self.running = true;

        info!("ENGINE: capture started");

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        info!("ENGINE: stopping capture");

        self.capture.stop()?;

        self.running = false;

        info!("ENGINE: capture stopped");

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn submit_frame(&mut self, frame: VideoFrame) -> Result<()> {
        debug!("ENGINE: manual submit_frame called");

        let packets = self.encoder.lock().unwrap().submit(frame)?;

        debug!("ENGINE: manual encoder produced {} packets", packets.len());

        let mut buffer = self.buffer.lock().unwrap();

        for packet in packets {
            debug!("ENGINE: manual packet timestamp={}", packet.timestamp);

            buffer.push(packet);
        }

        debug!("ENGINE: buffer size={}", buffer.len());

        Ok(())
    }

    pub fn save_clip(&self, seconds: usize) -> Result<String> {
        info!("ENGINE: saving clip {} seconds", seconds);

        let packets = self
            .buffer
            .lock()
            .unwrap()
            .get_last_seconds(seconds, self.config.framerate as usize);

        if packets.is_empty() {
            anyhow::bail!("Replay buffer is empty");
        }

        let filename = format!("clip_{}.h264", chrono::Utc::now().timestamp());

        let path = self.config.output_dir.join(filename);

        let mut file = std::fs::File::create(&path)?;

        use std::io::Write;

        for packet in packets {
            file.write_all(&packet.data)?;
        }

        info!("ENGINE: wrote {:?}", path);

        Ok(path.to_string_lossy().to_string())
    }
}
