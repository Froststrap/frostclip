use std::os::fd::AsRawFd;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use std::time::Duration;

use anyhow::{Result, anyhow};

use ashpd::desktop::{
    PersistMode,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};

use pipewire as pw;

use pipewire::{loop_::Timeout, spa::utils::Direction};

use crate::{
    capture::CaptureBackend,
    frame::{DmaBufFrame, VideoFrame},
};

pub struct LinuxCapture {
    pub running: Arc<AtomicBool>,
}

impl LinuxCapture {
    pub fn new() -> Result<Self> {
        log::info!("LINUX: creating LinuxCapture");

        Ok(Self {
            running: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl CaptureBackend for LinuxCapture {
    fn start(&mut self, callback: Box<dyn Fn(VideoFrame) + Send + Sync>) -> Result<()> {
        log::info!("LINUX: start called");

        self.running.store(true, Ordering::Relaxed);

        let running = Arc::clone(&self.running);

        let callback = Arc::new(Mutex::new(callback));

        std::thread::spawn(move || {
            log::info!("LINUX THREAD: started");

            let runtime = tokio::runtime::Runtime::new().expect("failed creating tokio runtime");

            runtime.block_on(async move {
                log::info!("LINUX ASYNC: entering capture");

                match run(running, callback).await {
                    Ok(_) => {
                        log::info!("LINUX ASYNC: exited normally");
                    }

                    Err(err) => {
                        log::error!("LINUX ERROR {:?}", err);
                    }
                }
            });
        });

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        log::info!("LINUX: stop called");

        self.running.store(false, Ordering::Relaxed);

        Ok(())
    }
}

pub async fn run(
    running: Arc<AtomicBool>,
    callback: Arc<Mutex<Box<dyn Fn(VideoFrame) + Send + Sync>>>,
) -> Result<()> {
    log::info!("PORTAL: creating screencast");

    let proxy = Screencast::new().await?;

    log::info!("PORTAL: created");

    let session = proxy.create_session(Default::default()).await?;

    log::info!("PORTAL: session created");

    let options = SelectSourcesOptions::default()
        .set_sources(SourceType::Monitor | SourceType::Window)
        .set_cursor_mode(CursorMode::Embedded)
        .set_persist_mode(PersistMode::ExplicitlyRevoked);

    log::info!("PORTAL: selecting sources");

    proxy.select_sources(&session, options).await?;

    log::info!("PORTAL: sources selected");

    let response = proxy
        .start(&session, None, Default::default())
        .await?
        .response()?;

    log::info!("PORTAL: started");

    let stream_info = response
        .streams()
        .first()
        .ok_or_else(|| anyhow!("No streams"))?;

    let node_id = stream_info.pipe_wire_node_id();

    log::info!("PIPEWIRE: node {}", node_id);

    let pw_fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await?;

    log::info!("PIPEWIRE: fd {}", pw_fd.as_raw_fd());

    pw::init();

    let main_loop = pw::main_loop::MainLoopBox::new(None)?;

    log::info!("PIPEWIRE: main loop created");

    let context = pw::context::ContextBox::new(&main_loop.loop_(), None)?;

    let core = context.connect_fd(pw_fd, None)?;

    log::info!("PIPEWIRE: core connected");

    let stream = pw::stream::StreamBox::new(
        &core,
        "frostclip-capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    log::info!("PIPEWIRE: stream created");

    let _state = stream
        .add_local_listener::<()>()
        .state_changed(|_stream, _data, old, new| {
            log::info!("PIPEWIRE STATE {:?} -> {:?}", old, new);
        })
        .register()?;

    let _listener = stream
        .add_local_listener::<()>()
        .process({
            let callback = Arc::clone(&callback);

            move |stream, _| {
                log::info!("PIPEWIRE PROCESS CALLBACK");

                let buffer = match stream.dequeue_buffer() {
                    Some(buffer) => buffer,
                    None => {
                        log::warn!("PIPEWIRE NO BUFFER");
                        return;
                    }
                };

                if buffer.is_null() {
                    log::warn!("PIPEWIRE NULL BUFFER");
                    return;
                }

                unsafe {
                    let spa_buffer = (*buffer).buffer;

                    if spa_buffer.is_null() {
                        log::warn!("PIPEWIRE NULL SPA BUFFER");
                    } else {
                        let datas = std::slice::from_raw_parts(
                            let datas = buffer.datas();,
                            (*spa_buffer).n_datas as usize,
                        );

                        log::info!("PIPEWIRE DATA BLOCKS {}", datas.len());

                        for data in datas {
                            log::info!("PIPEWIRE DATA type={} fd={}", data.type_, data.fd);

                            let frame = VideoFrame::LinuxDmaBuf(DmaBufFrame {
                                fd: data.fd as i32,
                                width: 0,
                                height: 0,
                                stride: 0,
                                offset: 0,
                                modifier: 0,
                                format: crate::frame::VideoFormat::Unknown,
                                timestamp: 0,
                            });

                            log::info!("PIPEWIRE CALLING ENGINE");

                            if let Ok(cb) = callback.lock() {
                                cb(frame);
                                log::info!("PIPEWIRE ENGINE RETURNED");
                            }

                            break;
                        }
                    }

                    stream.queue_raw_buffer(buffer);

                    log::debug!("PIPEWIRE BUFFER RETURNED");
                }
            }
        })
        .register()?;

    log::info!("PIPEWIRE: listener registered");

    // Build format using pipewire's macro builder
    let obj = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            pw::spa::param::format::MediaType::Video
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            pw::spa::param::format::MediaSubtype::Raw
        )
    );

    let values = vec![pw::spa::pod::Value::Object(obj)];

    let mut params = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &values[0],
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|e| anyhow!("Failed to serialize param pod: {:?}", e))?;

    let pod = pw::spa::pod::Pod::from_bytes(&params)
        .ok_or_else(|| anyhow!("Failed to parse pod bytes"))?;

    let mut param_ptrs = [pod];

    log::info!("PIPEWIRE: connecting stream");

    stream.connect(
        Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::DRIVER,
        &mut param_ptrs,
    )?;

    log::info!("PIPEWIRE: connected");

    while running.load(Ordering::Relaxed) {
        main_loop
            .loop_()
            .iterate(Timeout::Finite(Duration::from_millis(10)));
    }

    log::info!("PIPEWIRE: loop ended");

    Ok(())
}
