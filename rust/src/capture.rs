use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use xcap::Monitor;

use crate::buffer::ReplayBuffer;
use crate::encoder::VideoEncoder;
use crate::audio::AudioCapture;

pub struct CaptureEngine {
    framerate: u32,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    output_dir: PathBuf,
    audio_mode: String,
    running: Arc<AtomicBool>,
    buffer: Arc<Mutex<ReplayBuffer>>,
    encoder: Arc<Mutex<Option<VideoEncoder>>>,
    capture_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl CaptureEngine {
    pub fn new(
        framerate: u32,
        width: u32,
        height: u32,
        bitrate_kbps: u32,
        output_dir: &str,
        audio_mode: &str,
    ) -> Result<Self> {
        let output_dir_path = Path::new(output_dir).to_path_buf();
        std::fs::create_dir_all(&output_dir_path)?;

        ffmpeg_next::init()?;

        let codec = Self::detect_codec()?;
        log::info!("Using codec: {}", codec);

        let encoder = VideoEncoder::new(
            width as i32,
            height as i32,
            framerate as i32,
            bitrate_kbps as i32,
            &codec,
        )?;

        let buffer = ReplayBuffer::new(
            framerate as usize,
            60,
            width as usize,
            height as usize,
        );

        Ok(Self {
            framerate,
            width,
            height,
            bitrate_kbps,
            output_dir: output_dir_path,
            audio_mode: audio_mode.to_string(),
            running: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(buffer)),
            encoder: Arc::new(Mutex::new(Some(encoder))),
            capture_thread: Arc::new(Mutex::new(None)),
        })
    }

    fn detect_codec() -> Result<String> {
        if std::process::Command::new("nvidia-smi")
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok("nvenc".to_string());
        }
        if std::path::Path::new("/dev/dri/renderD128").exists() {
            return Ok("vaapi".to_string());
        }
        if std::process::Command::new("vulkaninfo").output() .is_ok() {
            return Ok("vulkan".to_string());
        }
        Ok("libx264".to_string())
    }

    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(anyhow!("already running"));
        }

        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let buffer = self.buffer.clone();
        let framerate = self.framerate;
        let width = self.width;
        let height = self.height;
        let audio_mode = self.audio_mode.clone();

        let handle = thread::spawn(move || {
            let mut audio_capture = if audio_mode != "off" {
                Some(AudioCapture::new(&audio_mode))
            } else {
                None
            };

            let frame_duration = Duration::from_secs_f64(1.0 / framerate as f64);
            let mut next_frame_time = std::time::Instant::now();

            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    log::error!("Failed to get monitors: {}", e);
                    return;
                }
            };

            let monitor = match monitors.first() {
                Some(m) => m,
                None => {
                    log::error!("No monitor found");
                    return;
                }
            };

            log::info!("Capturing monitor: {}x{}", monitor.width().unwrap_or(0), monitor.height().unwrap_or(0));

            while running.load(Ordering::SeqCst) {
                let now = std::time::Instant::now();
                if now < next_frame_time {
                    thread::sleep(next_frame_time - now);
                }
                next_frame_time += frame_duration;

                let frame = match monitor.capture_image() {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("Failed to capture frame: {}", e);
                        continue;
                    }
                };

                let frame_data = if (frame.width() as usize) != width as usize || (frame.height() as usize) != height as usize {
                    // TODO: resize using swscale
                    frame.into_raw()
                } else {
                    frame.into_raw()
                };

                if let Ok(mut buf) = buffer.lock() {
                    buf.push(frame_data);
                }

                if let Some(audio) = &mut audio_capture {
                    let _ = audio.capture();
                }
            }

            log::info!("Capture thread stopped");
        });

        let mut guard = self.capture_thread.lock().unwrap();
        *guard = Some(handle);

        log::info!("Capture started at {} FPS", self.framerate);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.capture_thread.lock().unwrap().take() {
            let _ = handle.join();
        }
        log::info!("Capture stopped");
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn save_clip(&mut self, seconds: u32) -> Result<String> {
        let frames_needed = (seconds * self.framerate) as usize;
        let buffer = self.buffer.lock().unwrap();
        let frames = buffer.get_last_frames(frames_needed);
        if frames.is_empty() {
            return Err(anyhow!("No frames in buffer"));
        }

        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let filename = format!("clip_{}s_{}.mp4", seconds, timestamp);
        let output_path = self.output_dir.join(&filename);

        let encoder = self.encoder.lock().unwrap();
        let encoder = encoder.as_ref().ok_or_else(|| anyhow!("No encoder"))?;
        encoder.encode_frames(&frames, &output_path)?;

        log::info!("Clip saved: {}", output_path.display());
        Ok(output_path.to_string_lossy().to_string())
    }

    pub fn save_clip_with_metadata(
        &mut self,
        seconds: u32,
        game_name: &str,
        event_type: &str,
    ) -> Result<String> {
        let path = self.save_clip(seconds)?;
        let metadata_path = Path::new(&path).with_extension("json");
        let metadata = serde_json::json!({
            "game": game_name,
            "event": event_type,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "duration_seconds": seconds,
        });
        std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;
        Ok(path)
    }
}

impl Drop for CaptureEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
