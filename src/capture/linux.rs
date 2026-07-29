use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::info;

use pipewire as pw;

use crate::{
    capture::{CaptureBackend, CaptureInfo},
    frame::VideoFrame,
};

pub struct LinuxCapture {
    running: Arc<AtomicBool>,
}

impl LinuxCapture {
    pub fn new() -> Result<Self, ()> {
        pw::init();

        let main_loop = pw::main_loop::MainLoopBox::new(None).map_err(|_| ())?;

        info!("PIPEWIRE: main loop created");

        let context = pw::context::ContextBox::new(main_loop.loop_(), None).map_err(|_| ())?;

        info!("PIPEWIRE: context created");

        let _core = context.connect(None).map_err(|_| ())?;

        info!("PIPEWIRE: core connected");

        Ok(Self {
            running: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl CaptureBackend for LinuxCapture {
    fn info(&self) -> CaptureInfo {
        CaptureInfo {
            width: 0,
            height: 0,
        }
    }

    fn start(&mut self, _callback: Box<dyn Fn(VideoFrame) + Send + Sync>) -> Result<(), ()> {
        self.running.store(true, Ordering::Relaxed);

        info!("PIPEWIRE: capture not implemented yet");

        Ok(())
    }

    fn stop(&mut self) -> Result<(), ()> {
        self.running.store(false, Ordering::Relaxed);

        Ok(())
    }
}
