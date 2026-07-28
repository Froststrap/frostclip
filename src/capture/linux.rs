use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use xcap::Monitor;

use crate::{
    capture::{CaptureBackend, CaptureInfo},
    frame::{VideoFormat, VideoFrame},
};

pub struct LinuxCapture {
    running: Arc<AtomicBool>,
}

impl LinuxCapture {
    pub fn new() -> Result<Self, ()> {
        Ok(Self {
            running: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl CaptureBackend for LinuxCapture {
    fn info(&self) -> CaptureInfo {
        let monitors = Monitor::all().expect("XCAP: failed getting monitors");

        let monitor = monitors.first().expect("XCAP: no monitors available");

        CaptureInfo {
            width: monitor.width().expect("XCAP: failed getting monitor width"),

            height: monitor
                .height()
                .expect("XCAP: failed getting monitor height"),
        }
    }

    fn start(&mut self, callback: Box<dyn Fn(VideoFrame) + Send + Sync>) -> Result<(), ()> {
        self.running.store(true, Ordering::Relaxed);

        let running = Arc::clone(&self.running);

        let callback = Arc::new(Mutex::new(callback));

        std::thread::spawn(move || {
            log::info!("XCAP: capture thread started");

            let monitors = match Monitor::all() {
                Ok(monitors) => monitors,

                Err(err) => {
                    log::error!("XCAP: failed getting monitors {:?}", err);
                    return;
                }
            };

            let monitor = match monitors.first() {
                Some(monitor) => monitor,

                None => {
                    log::error!("XCAP: no monitors available");
                    return;
                }
            };

            let width = match monitor.width() {
                Ok(width) => width,

                Err(err) => {
                    log::error!("XCAP: failed width {:?}", err);
                    return;
                }
            };

            let height = match monitor.height() {
                Ok(height) => height,

                Err(err) => {
                    log::error!("XCAP: failed height {:?}", err);
                    return;
                }
            };

            log::info!("XCAP: capturing {}x{}", width, height);

            let start_time = Instant::now();

            let mut frame_counter = 0u32;
            let mut fps_timer = Instant::now();

            while running.load(Ordering::Relaxed) {
                let capture_start = Instant::now();

                match monitor.capture_image() {
                    Ok(image) => {
                        log::info!("XCAP capture took {:?}", capture_start.elapsed());

                        frame_counter += 1;

                        if fps_timer.elapsed() >= Duration::from_secs(1) {
                            log::info!("XCAP capture FPS: {}", frame_counter);

                            frame_counter = 0;
                            fps_timer = Instant::now();
                        }

                        let timestamp = start_time.elapsed().as_micros() as u64;

                        let frame = VideoFrame::Cpu {
                            data: image.into_raw(),

                            width,

                            height,

                            stride: width * 4,

                            format: VideoFormat::Rgba,

                            timestamp,
                        };

                        if let Ok(cb) = callback.lock() {
                            cb(frame);
                        }
                    }

                    Err(err) => {
                        log::error!("XCAP: capture failed {:?}", err);
                    }
                }
            }

            log::info!("XCAP: capture thread stopped");
        });

        Ok(())
    }

    fn stop(&mut self) -> Result<(), ()> {
        self.running.store(false, Ordering::Relaxed);

        Ok(())
    }
}
