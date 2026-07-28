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
use crate::muxer::MuxerFactory;

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

            framerate: config.framerate,
        })?;

        info!("ENGINE: encoder created");

        let video_info = encoder
            .video_info()
            .ok_or_else(|| anyhow::anyhow!("Missing video info"))?;

        let timebase = video_info.time_base_den as i64;

        let buffer = ReplayBuffer::new(config.replay_buffer_seconds as usize, timebase);

        info!(
            "ENGINE: replay buffer created duration={}s timebase={}",
            config.replay_buffer_seconds, timebase
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
                    "ENGINE: pushing packet pts={} dts={} keyframe={}",
                    packet.pts, packet.dts, packet.is_keyframe
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

        std::thread::sleep(std::time::Duration::from_millis(500));

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
            debug!(
                "ENGINE: manual packet pts={} dts={}",
                packet.pts, packet.dts
            );

            buffer.push(packet);
        }

        debug!("ENGINE: buffer size={}", buffer.len());

        Ok(())
    }

    pub fn save_clip(&self) -> Result<String> {
        self.save_clip_with_duration(self.config.default_clip_seconds as usize)
    }

    pub fn save_clip_with_duration(&self, seconds: usize) -> Result<String> {
        info!("ENGINE: saving clip {} seconds", seconds);

        let buffer = self.buffer.lock().unwrap();

        let packets = buffer.get_last_seconds(seconds);
        let timebase = buffer.timebase();

        info!("BUFFER: requested={}s packets={}", seconds, packets.len());

        if let (Some(first), Some(last)) = (packets.first(), packets.last()) {
            info!(
                "BUFFER RANGE: first_pts={} last_pts={} duration={}s",
                first.pts,
                last.pts,
                (last.pts - first.pts) as f64 / timebase as f64
            );
        }

        drop(buffer);

        if packets.is_empty() {
            anyhow::bail!("Replay buffer is empty");
        }

        let muxer_factory = crate::muxer::Mp4MuxerFactory;

        let filename = format!(
            "clip_{}.{}",
            chrono::Utc::now().timestamp(),
            muxer_factory.extension()
        );

        let path = self.config.output_dir.join(filename);

        let encoder = self.encoder.lock().unwrap();

        let video_info = encoder
            .video_info()
            .ok_or_else(|| anyhow::anyhow!("Missing video info"))?;

        drop(encoder);

        let mut muxer = muxer_factory.create(path.to_str().unwrap(), &video_info)?;

        for packet in packets {
            muxer.write(&packet)?;
        }

        muxer.finish()?;

        info!("ENGINE: wrote {:?}", path);

        Ok(path.to_string_lossy().to_string())
    }
}
