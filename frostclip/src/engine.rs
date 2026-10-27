use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use config::{AppState, Config};
use tracing::{debug, error, info, warn};

use crate::audio::{AudioCapture, AudioSource};
use crate::buffer::{AudioBuffer, ReplayBuffer};
use crate::capture::Capture;
use crate::encoder::audio::AudioEncoder;
use crate::encoder::{AudioInfo, EncodedPacket, Encoder, EncoderConfig, create_encoder};
use crate::frame::VideoFrame;
use crate::muxer::MuxerFactory;

#[derive(Debug)]
pub enum EngineError {
    OutputDir(String),
    Capture,
    Encoder,
    Muxer,
    MissingVideoInfo,
    EmptyBuffer,
    InvalidPath,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::OutputDir(msg) => write!(f, "output directory error: {}", msg),
            EngineError::Capture => write!(f, "capture error"),
            EngineError::Encoder => write!(f, "encoder error"),
            EngineError::Muxer => write!(f, "muxer error"),
            EngineError::MissingVideoInfo => write!(f, "encoder has no video info"),
            EngineError::EmptyBuffer => write!(f, "replay buffer is empty"),
            EngineError::InvalidPath => write!(f, "output path is not valid utf-8"),
        }
    }
}

impl std::error::Error for EngineError {}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const AUDIO_CHANNELS: u16 = 2;

struct AudioPipeline {
    capture: AudioCapture,
    encoder: Arc<Mutex<AudioEncoder>>,
    buffer: Arc<Mutex<AudioBuffer>>,
    info: AudioInfo,
    worker: Option<std::thread::JoinHandle<()>>,
}

fn create_audio(config: &Config) -> Option<AudioPipeline> {
    if !config.audio_enabled {
        info!("ENGINE: audio disabled");
        return None;
    }

    let encoder =
        match AudioEncoder::new(AUDIO_SAMPLE_RATE, AUDIO_CHANNELS, config.audio_bitrate_kbps) {
            Ok(encoder) => encoder,
            Err(_) => {
                warn!("ENGINE: audio encoder init failed, video only");
                return None;
            }
        };

    let capture = match AudioCapture::new(AudioSource::Desktop) {
        Ok(capture) => capture,
        Err(err) => {
            warn!("ENGINE: audio capture init failed: {}, video only", err);
            return None;
        }
    };

    let info = encoder.info();
    let buffer = AudioBuffer::new(config.replay_buffer_seconds as usize, AUDIO_SAMPLE_RATE);

    Some(AudioPipeline {
        capture,
        encoder: Arc::new(Mutex::new(encoder)),
        buffer: Arc::new(Mutex::new(buffer)),
        info,
        worker: None,
    })
}

pub struct CaptureEngine {
    capture: Capture,
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    buffer: Arc<Mutex<ReplayBuffer>>,
    config: Config,
    running: Arc<Mutex<bool>>,
    audio: Option<AudioPipeline>,
}

impl CaptureEngine {
    pub async fn new(config: Config, mut state: AppState) -> Result<Self, EngineError> {
        info!("ENGINE: creating capture engine");

        config
            .ensure_output_dir()
            .map_err(|err| EngineError::OutputDir(err.to_string()))?;

        info!("ENGINE: output directory {:?}", config.output_dir);

        let capture = Capture::new(&mut state)
            .await
            .map_err(|_| EngineError::Capture)?;
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
            bitrate_kbps: config.bitrate_kbps,
        })
        .map_err(|_| EngineError::Encoder)?;

        info!("ENGINE: encoder created");

        let video_info = encoder.video_info().ok_or(EngineError::MissingVideoInfo)?;

        let timebase = video_info.time_base_den as i64;

        let buffer = ReplayBuffer::new(config.replay_buffer_seconds as usize, timebase);

        info!(
            "ENGINE: replay buffer created duration={}s timebase={}",
            config.replay_buffer_seconds, timebase
        );

        let audio = create_audio(&config);

        Ok(Self {
            capture,
            encoder: Arc::new(Mutex::new(encoder)),
            buffer: Arc::new(Mutex::new(buffer)),
            config,
            running: Arc::new(Mutex::new(false)),
            audio,
        })
    }

    pub fn start(&mut self) -> Result<(), EngineError> {
        info!("ENGINE: start requested");

        let encoder = Arc::clone(&self.encoder);
        let buffer = Arc::clone(&self.buffer);
        let running = Arc::clone(&self.running);

        let frame_count = Arc::new(Mutex::new(0u64));
        let fps_start = Arc::new(Mutex::new(std::time::Instant::now()));

        let fps_count = Arc::clone(&frame_count);
        let fps_timer = Arc::clone(&fps_start);
        let accepts_dmabuf = lock(&self.encoder).accepts_dmabuf();

        *lock(&running) = true;

        self.capture
            .start(
                Box::new(move |frame| {
                    if !*lock(&running) {
                        return;
                    }

                    let _elapsed = {
                        let mut count = lock(&fps_count);
                        *count += 1;

                        let mut timer = lock(&fps_timer);
                        let elapsed = timer.elapsed();

                        if elapsed.as_secs_f64() >= 1.0 {
                            info!(
                                "ENGINE: capture FPS={:.2} frames={}",
                                *count as f64 / elapsed.as_secs_f64(),
                                *count
                            );

                            *count = 0;
                            *timer = std::time::Instant::now();
                        }

                        elapsed
                    };

                    let t0 = std::time::Instant::now();

                    let packets = match lock(&encoder).submit(frame) {
                        Ok(packets) => packets,
                        Err(_) => {
                            error!("ENGINE: encoder submit failed");
                            return;
                        }
                    };

                    debug!(
                        "ENGINE: encoder submit={:.2}ms packets={}",
                        t0.elapsed().as_secs_f64() * 1000.0,
                        packets.len()
                    );

                    let mut buffer = lock(&buffer);

                    for packet in packets {
                        buffer.push(packet);
                    }
                }),
                accepts_dmabuf,
            )
            .map_err(|_| EngineError::Capture)?;

        let tick = std::time::Duration::from_micros(1_000_000 / self.config.framerate as u64 / 4);
        let idle_encoder = Arc::clone(&self.encoder);
        let idle_buffer = Arc::clone(&self.buffer);
        let idle_running = Arc::clone(&self.running);

        std::thread::spawn(move || {
            while *lock(&idle_running) {
                std::thread::sleep(tick);

                let packets = match lock(&idle_encoder).repeat_last() {
                    Ok(packets) => packets,
                    Err(_) => {
                        error!("ENGINE: repeat_last failed");
                        continue;
                    }
                };

                if packets.is_empty() {
                    continue;
                }

                let mut buffer = lock(&idle_buffer);

                for packet in packets {
                    buffer.push(packet);
                }
            }
        });

        if let Some(audio) = self.audio.as_mut() {
            match audio.capture.start() {
                Ok(receiver) => {
                    let encoder = Arc::clone(&audio.encoder);
                    let buffer = Arc::clone(&audio.buffer);

                    audio.worker = Some(std::thread::spawn(move || {
                        while let Ok(frame) = receiver.recv() {
                            let packets = match lock(&encoder).submit(frame) {
                                Ok(packets) => packets,
                                Err(_) => {
                                    error!("ENGINE: audio encoder submit failed");
                                    continue;
                                }
                            };

                            let mut buffer = lock(&buffer);

                            for packet in packets {
                                buffer.push(packet);
                            }
                        }
                    }));

                    info!("ENGINE: audio capture started");
                }
                Err(err) => {
                    warn!("ENGINE: audio capture start failed: {}, video only", err);
                }
            }
        }

        info!("ENGINE: capture started");

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), EngineError> {
        info!("ENGINE: stopping capture");

        *lock(&self.running) = false;
        self.capture.stop().map_err(|_| EngineError::Capture)?;

        if let Some(audio) = self.audio.as_mut() {
            if let Err(err) = audio.capture.stop() {
                warn!("ENGINE: audio stop failed: {}", err);
            }

            if let Some(worker) = audio.worker.take() {
                let _ = worker.join();
            }
        }

        self.drain_encoder()?;
        self.drain_audio();

        info!("ENGINE: capture stopped");

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        *lock(&self.running)
    }

    pub fn submit_frame(&mut self, frame: VideoFrame) -> Result<(), EngineError> {
        debug!("ENGINE: manual submit_frame called");

        let packets = lock(&self.encoder)
            .submit(frame)
            .map_err(|_| EngineError::Encoder)?;

        debug!("ENGINE: manual encoder produced {} packets", packets.len());

        let mut buffer = lock(&self.buffer);

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

    pub fn save_clip(&self) -> Result<String, EngineError> {
        self.save_clip_with_duration(self.config.default_clip_seconds as usize)
    }

    pub fn save_clip_with_duration(&self, seconds: usize) -> Result<String, EngineError> {
        info!("ENGINE: saving clip {} seconds", seconds);

        let buffer = lock(&self.buffer);

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
            return Err(EngineError::EmptyBuffer);
        }

        let muxer_factory = crate::muxer::Mp4MuxerFactory;

        let filename = format!(
            "clip_{}.{}",
            chrono::Utc::now().timestamp(),
            muxer_factory.extension()
        );

        let path = self.config.output_dir.join(filename);
        let path_str = path.to_str().ok_or(EngineError::InvalidPath)?;

        let video_info = lock(&self.encoder)
            .video_info()
            .ok_or(EngineError::MissingVideoInfo)?;

        let video_num = video_info.time_base_num as i64;
        let video_den = video_info.time_base_den as i64;
        let video_start_us = packets[0].pts * 1_000_000 * video_num / video_den;

        let (audio_info, audio_packets) = match &self.audio {
            Some(audio) => {
                let audio_packets = lock(&audio.buffer).get_from_us(video_start_us);

                if audio_packets.is_empty() {
                    (None, audio_packets)
                } else {
                    (Some(audio.info.clone()), audio_packets)
                }
            }
            None => (None, Vec::new()),
        };

        info!(
            "ENGINE: clip video_packets={} audio_packets={}",
            packets.len(),
            audio_packets.len()
        );

        let mut merged: Vec<(i64, EncodedPacket)> = Vec::new();

        for packet in packets {
            merged.push((packet.dts * 1_000_000 * video_num / video_den, packet));
        }

        for packet in audio_packets {
            merged.push((packet.dts * 1_000_000 / AUDIO_SAMPLE_RATE as i64, packet));
        }

        merged.sort_by_key(|(time_us, _)| *time_us);

        let mut muxer = muxer_factory
            .create(path_str, &video_info, audio_info.as_ref())
            .map_err(|_| EngineError::Muxer)?;

        for (_, packet) in merged {
            muxer.write(&packet).map_err(|_| EngineError::Muxer)?;
        }

        muxer.finish().map_err(|_| EngineError::Muxer)?;

        info!("ENGINE: wrote {:?}", path);

        Ok(path.to_string_lossy().to_string())
    }

    fn drain_encoder(&self) -> Result<(), EngineError> {
        let packets = lock(&self.encoder)
            .flush()
            .map_err(|_| EngineError::Encoder)?;

        if packets.is_empty() {
            return Ok(());
        }

        let mut buffer = lock(&self.buffer);

        for packet in packets {
            debug!(
                "ENGINE: flushing packet pts={} dts={} keyframe={}",
                packet.pts, packet.dts, packet.is_keyframe
            );

            buffer.push(packet);
        }

        Ok(())
    }

    fn drain_audio(&self) {
        let Some(audio) = &self.audio else {
            return;
        };

        let packets = match lock(&audio.encoder).flush() {
            Ok(packets) => packets,
            Err(_) => {
                warn!("ENGINE: audio flush failed");
                return;
            }
        };

        let mut buffer = lock(&audio.buffer);

        for packet in packets {
            buffer.push(packet);
        }
    }
}
