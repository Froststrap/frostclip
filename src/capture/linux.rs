use crate::capture::{CaptureBackend, CaptureInfo};
use crate::portal::screencast::PortalStream;

use pipewire as pw;
use tracing::{error, info};

pub struct LinuxCapture {
    portal: Option<PortalStream>,
    main_loop: Option<pw::main_loop::MainLoopRc>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl LinuxCapture {
    pub async fn new(restore_token: Option<&str>) -> Result<Self, ()> {
        let portal = crate::portal::screencast::create(restore_token)
            .await
            .map_err(|err| {
                error!("PORTAL creation failed: {}", err);
            })?;

        Ok(Self {
            portal: Some(portal),
            main_loop: None,
            thread_handle: None,
        })
    }

    fn start_capture(
        &mut self,
        callback: Box<dyn Fn(crate::frame::VideoFrame) + Send + Sync>,
    ) -> Result<(), ()> {
        let portal = self.portal.take().ok_or(())?;

        let handle = std::thread::spawn(move || {
            pw::init();

            let main_loop = match pw::main_loop::MainLoopRc::new(None) {
                Ok(loop_) => loop_,
                Err(err) => {
                    error!("PIPEWIRE: failed to create main loop: {:?}", err);
                    return;
                }
            };

            let node_id = match portal.streams.first() {
                Some(stream) => stream.pipe_wire_node_id(),
                None => {
                    error!("PORTAL: no valid streams available");
                    return;
                }
            };

            let context = match pw::context::ContextRc::new(&main_loop, None) {
                Ok(ctx) => ctx,
                Err(err) => {
                    error!("PIPEWIRE: failed to create context: {:?}", err);
                    return;
                }
            };

            let _core = match context.connect_fd_rc(portal.pipewire_fd, None) {
                Ok(core) => core,
                Err(err) => {
                    error!("PIPEWIRE: failed to connect portal fd: {:?}", err);
                    return;
                }
            };

            info!("PIPEWIRE: connected to portal node {}", node_id);

            // TODO:
            // Create StreamRc here.

            info!("PIPEWIRE: running main loop");

            main_loop.run();

            info!("PIPEWIRE: loop stopped cleanly");
        });

        self.thread_handle = Some(handle);

        Ok(())
    }

    fn stop_capture(&mut self) -> Result<(), ()> {
        if let Some(main_loop) = self.main_loop.take() {
            main_loop.quit();
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        info!("Linux capture stopped");

        Ok(())
    }
}

impl CaptureBackend for LinuxCapture {
    fn start(
        &mut self,
        callback: Box<dyn Fn(crate::frame::VideoFrame) + Send + Sync>,
    ) -> Result<(), ()> {
        self.start_capture(callback)
    }

    fn info(&self) -> CaptureInfo {
        self.portal
            .as_ref()
            .and_then(|portal| portal.streams.first())
            .and_then(|stream| stream.size())
            .map(|(width, height)| CaptureInfo {
                width: width as u32,
                height: height as u32,
            })
            .unwrap_or(CaptureInfo {
                width: 0,
                height: 0,
            })
    }

    fn stop(&mut self) -> Result<(), ()> {
        self.stop_capture()
    }
}
