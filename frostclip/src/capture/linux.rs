use crate::capture::{CaptureBackend, CaptureInfo};
use crate::frame::{DmaBufFrame, VideoFormat, VideoFrame};
use crate::portal::screencast::PortalStream;

use pipewire as pw;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::format_utils;
use pw::spa::param::video::{VideoFormat as SpaVideoFormat, VideoInfoRaw};
use pw::spa::pod::{Pod, object, property};
use pw::spa::utils::{Fraction, Rectangle};
use std::time::Instant;
use tracing::{debug, error, info, warn};

type FrameCallback = Box<dyn Fn(VideoFrame) + Send + Sync>;

struct StreamState {
    format: VideoInfoRaw,
    negotiated: bool,
    callback: FrameCallback,
    started: Instant,
    expected: (u32, u32),
    logged_no_map: bool,
    logged_type: bool,
    frames: u64,
    accepts_dmabuf: bool,
}

pub struct LinuxCapture {
    portal: Option<PortalStream>,
    restore_token: Option<String>,
    quit_sender: Option<pw::channel::Sender<()>>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
    info: CaptureInfo,
}

impl LinuxCapture {
    pub async fn new(
        restore_token: Option<&str>,
        state: &mut crate::config::AppState,
    ) -> Result<Self, ()> {
        let portal = crate::portal::screencast::create(restore_token)
            .await
            .map_err(|err| {
                error!("PORTAL creation failed: {}", err);
            })?;

        let restore_token = portal.restore_token.clone();
        if let Some(token) = &restore_token {
            state.set_session_token(token.clone());

            if let Err(err) = state.save() {
                error!("STATE: failed to save restore token: {}", err);
            } else {
                info!("STATE: saved restore token");
            }
        }

        let info = portal
            .streams
            .first()
            .and_then(|stream| stream.size())
            .map(|(width, height)| CaptureInfo {
                width: width as u32,
                height: height as u32,
            })
            .unwrap_or(CaptureInfo {
                width: 0,
                height: 0,
            });

        info!("CAPTURE: portal size {}x{}", info.width, info.height);

        Ok(Self {
            portal: Some(portal),
            restore_token,
            quit_sender: None,
            thread_handle: None,
            info,
        })
    }

    fn build_format_params(with_modifiers: bool) -> Result<Vec<u8>, ()> {
        use pw::spa::pod::{ChoiceValue, Property, PropertyFlags, Value};
        use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags};

        let mut obj = object! {
            pw::spa::utils::SpaTypes::ObjectParamFormat,
            pw::spa::param::ParamType::EnumFormat,
            property!(FormatProperties::MediaType, Id, MediaType::Video),
            property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            property!(
                FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                SpaVideoFormat::BGRx,
                SpaVideoFormat::BGRx,
                SpaVideoFormat::BGRA,
                SpaVideoFormat::RGBx,
                SpaVideoFormat::RGBA
            ),
            property!(
                FormatProperties::VideoSize,
                Choice,
                Range,
                Rectangle,
                Rectangle {
                    width: 1920,
                    height: 1080
                },
                Rectangle {
                    width: 1,
                    height: 1
                },
                Rectangle {
                    width: 8192,
                    height: 8192
                }
            ),
            property!(
                FormatProperties::VideoFramerate,
                Choice,
                Range,
                Fraction,
                Fraction { num: 60, denom: 1 },
                Fraction { num: 0, denom: 1 },
                Fraction {
                    num: 1000,
                    denom: 1
                }
            ),
        };

        if with_modifiers {
            obj.properties.insert(
                3,
                Property {
                    key: FormatProperties::VideoModifier.as_raw(),
                    flags: PropertyFlags::MANDATORY | PropertyFlags::DONT_FIXATE,
                    value: Value::Choice(ChoiceValue::Long(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Enum {
                            default: 0,
                            alternatives: vec![0, 0x00ff_ffff_ffff_ffff],
                        },
                    ))),
                },
            );
        }

        pw::spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &pw::spa::pod::Value::Object(obj),
        )
        .map(|(cursor, _)| cursor.into_inner())
        .map_err(|err| {
            error!("PIPEWIRE: failed to serialize format params: {:?}", err);
        })
    }

    fn build_buffer_params(allow_dmabuf: bool) -> Result<Vec<u8>, ()> {
        use pw::spa::pod::{ChoiceValue, Object, Property, PropertyFlags, Value};
        use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags};

        let obj = Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamBuffers.as_raw(),
            id: pw::spa::param::ParamType::Buffers.as_raw(),
            properties: vec![
                Property {
                    key: pw::spa::sys::SPA_PARAM_BUFFERS_buffers,
                    flags: PropertyFlags::empty(),
                    value: Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Range {
                            default: 8,
                            min: 4,
                            max: 16,
                        },
                    ))),
                },
                Property {
                    key: pw::spa::sys::SPA_PARAM_BUFFERS_dataType,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(
                        (1 << pw::spa::sys::SPA_DATA_MemPtr)
                            | (1 << pw::spa::sys::SPA_DATA_MemFd)
                            | if allow_dmabuf {
                                1 << pw::spa::sys::SPA_DATA_DmaBuf
                            } else {
                                0
                            },
                    ),
                },
            ],
        };

        pw::spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &Value::Object(obj),
        )
        .map(|(cursor, _)| cursor.into_inner())
        .map_err(|err| {
            error!("PIPEWIRE: failed to serialize buffer params: {:?}", err);
        })
    }

    fn start_capture(&mut self, callback: FrameCallback, accepts_dmabuf: bool) -> Result<(), ()> {
        let portal = self.portal.take().ok_or(())?;
        let buffer_bytes = Self::build_buffer_params(accepts_dmabuf)?;
        let modifier_bytes = Self::build_format_params(true)?;
        let plain_bytes = Self::build_format_params(false)?;
        let expected = (self.info.width, self.info.height);

        let (quit_sender, quit_receiver) = pw::channel::channel::<()>();
        self.quit_sender = Some(quit_sender);

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

            let core = match context.connect_fd_rc(portal.pipewire_fd.try_clone().unwrap(), None) {
                Ok(core) => core,
                Err(err) => {
                    error!("PIPEWIRE: failed to connect portal fd: {:?}", err);
                    return;
                }
            };

            info!("PIPEWIRE: connected to portal node {}", node_id);

            let quit_loop = main_loop.clone();
            let _quit_attached = quit_receiver.attach(main_loop.loop_(), move |_| {
                quit_loop.quit();
            });

            let props = pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            };

            let stream = match pw::stream::StreamRc::new(core.clone(), "frostclip-capture", props) {
                Ok(stream) => stream,
                Err(err) => {
                    error!("PIPEWIRE: failed to create stream: {:?}", err);
                    return;
                }
            };

            let state = StreamState {
                format: VideoInfoRaw::new(),
                negotiated: false,
                callback,
                started: Instant::now(),
                expected,
                logged_no_map: false,
                logged_type: false,
                frames: 0,
                accepts_dmabuf,
            };

            let _listener = match stream
                .add_local_listener_with_user_data(state)
                .state_changed(|_, _, old, new| {
                    info!("PIPEWIRE: stream state {:?} -> {:?}", old, new);
                })
                .param_changed(|_, state, id, param| {
                    let Some(param) = param else {
                        return;
                    };

                    if id != pw::spa::param::ParamType::Format.as_raw() {
                        return;
                    }

                    let (media_type, media_subtype) = match format_utils::parse_format(param) {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                        return;
                    }

                    if state.format.parse(param).is_err() {
                        error!("PIPEWIRE: failed to parse negotiated video format");
                        return;
                    }

                    state.negotiated = true;

                    let size = state.format.size();
                    let fr = state.format.framerate();
                    let max_fr = state.format.max_framerate();
                    info!(
                        "PIPEWIRE: negotiated format={:?} size={}x{} fps={}/{} max={}/{}",
                        state.format.format(),
                        size.width,
                        size.height,
                        fr.num,
                        fr.denom,
                        max_fr.num,
                        max_fr.denom
                    );

                    if (size.width, size.height) != state.expected {
                        warn!(
                            "PIPEWIRE: negotiated size {}x{} differs from portal size {}x{}",
                            size.width, size.height, state.expected.0, state.expected.1
                        );
                    }

                    info!("PIPEWIRE: modifier=0x{:x}", state.format.modifier());
                })
                .process(|stream, state| {
                    let Some(mut buffer) = stream.dequeue_buffer() else {
                        return;
                    };

                    if !state.negotiated {
                        return;
                    }

                    let datas = buffer.datas_mut();
                    let Some(data) = datas.first_mut() else {
                        return;
                    };

                    let chunk_size = data.chunk().size() as usize;
                    let chunk_offset = data.chunk().offset() as usize;
                    let chunk_stride = data.chunk().stride();

                    if !state.logged_type {
                        info!(
                            "PIPEWIRE: buffer type={:?} chunk size={} offset={} stride={}",
                            data.type_(),
                            chunk_size,
                            chunk_offset,
                            chunk_stride
                        );
                        state.logged_type = true;
                    }

                    if chunk_size == 0 {
                        debug!("PIPEWIRE: empty chunk, skipping");
                        return;
                    }

                    if data.type_() == pw::spa::buffer::DataType::DmaBuf {
                        let size = state.format.size();
                        let raw_fd = data.fd();
                        let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(raw_fd) };
                        let owned = match borrowed.try_clone_to_owned() {
                            Ok(fd) => fd,
                            Err(err) => {
                                error!("PIPEWIRE: failed to dup dma-buf fd: {}", err);
                                return;
                            }
                        };

                        let raw_format = state.format.format().as_raw();
                        let format = if raw_format == SpaVideoFormat::BGRx.as_raw()
                            || raw_format == SpaVideoFormat::BGRA.as_raw()
                        {
                            VideoFormat::Bgra
                        } else {
                            VideoFormat::Rgba
                        };

                        let fourcc: u32 = if raw_format == SpaVideoFormat::BGRx.as_raw() {
                            0x3432_5258
                        } else if raw_format == SpaVideoFormat::BGRA.as_raw() {
                            0x3432_5241
                        } else if raw_format == SpaVideoFormat::RGBx.as_raw() {
                            0x3432_4258
                        } else {
                            0x3432_4241
                        };

                        let stride = if chunk_stride > 0 {
                            chunk_stride as u32
                        } else {
                            size.width * 4
                        };

                        state.frames += 1;

                        if state.frames == 1 || state.frames % 60 == 0 {
                            info!(
                                "PIPEWIRE: frame #{} dma-buf {}x{} t={:.2}s",
                                state.frames,
                                size.width,
                                size.height,
                                state.started.elapsed().as_secs_f64()
                            );
                        }

                        let frame = VideoFrame::LinuxDmaBuf(DmaBufFrame {
                            fd: owned,
                            width: size.width,
                            height: size.height,
                            stride,
                            offset: chunk_offset as u32,
                            modifier: state.format.modifier(),
                            fourcc,
                            format,
                            timestamp: state.started.elapsed().as_micros() as u64,
                        });

                        drop(buffer);

                        if state.accepts_dmabuf {
                            (state.callback)(frame);
                            return;
                        }

                        let VideoFrame::LinuxDmaBuf(dma) = frame else {
                            return;
                        };

                        let t_read = Instant::now();
                        match dma.into_cpu() {
                            Ok(cpu) => {
                                let read_us = t_read.elapsed().as_micros();
                                let t_cb = Instant::now();
                                (state.callback)(cpu);
                                let cb_us = t_cb.elapsed().as_micros();
                                if state.frames % 60 == 0 {
                                    info!(
                                        "PIPEWIRE: timing read={}us callback={}us",
                                        read_us, cb_us
                                    );
                                }
                            }
                            Err(err) => error!("PIPEWIRE: dma-buf CPU read failed: {}", err),
                        }
                        return;
                        return;
                    }

                    let Some(bytes) = data.data() else {
                        if !state.logged_no_map {
                            warn!("PIPEWIRE: buffer not CPU-mapped (dma-buf?), cannot read");
                            state.logged_no_map = true;
                        }
                        return;
                    };

                    let size = state.format.size();
                    let width = size.width as usize;
                    let height = size.height as usize;

                    if width == 0 || height == 0 {
                        return;
                    }

                    let src_stride = if chunk_stride > 0 {
                        chunk_stride as usize
                    } else {
                        width * 4
                    };

                    let needed = chunk_offset + src_stride * (height - 1) + width * 4;
                    if bytes.len() < needed {
                        warn!(
                            "PIPEWIRE: buffer too small have={} need={}",
                            bytes.len(),
                            needed
                        );
                        return;
                    }

                    let raw_format = state.format.format().as_raw();
                    let swap = raw_format == SpaVideoFormat::BGRx.as_raw()
                        || raw_format == SpaVideoFormat::BGRA.as_raw();

                    let row_bytes = width * 4;
                    let mut out = vec![0u8; row_bytes * height];

                    for y in 0..height {
                        let src_start = chunk_offset + y * src_stride;
                        let src = &bytes[src_start..src_start + row_bytes];
                        let dst = &mut out[y * row_bytes..(y + 1) * row_bytes];

                        if swap {
                            for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                                d[0] = s[2];
                                d[1] = s[1];
                                d[2] = s[0];
                                d[3] = 255;
                            }
                        } else {
                            dst.copy_from_slice(src);
                        }
                    }

                    state.frames += 1;
                    if state.frames == 1 || state.frames % 60 == 0 {
                        info!(
                            "PIPEWIRE: frame #{} {}x{} t={:.2}s",
                            state.frames,
                            width,
                            height,
                            state.started.elapsed().as_secs_f64()
                        );
                    }

                    let frame = VideoFrame::Cpu {
                        data: out,
                        width: size.width,
                        height: size.height,
                        stride: row_bytes as u32,
                        format: VideoFormat::Rgba,
                        timestamp: state.started.elapsed().as_micros() as u64,
                    };

                    drop(buffer);

                    (state.callback)(frame);
                })
                .register()
            {
                Ok(listener) => listener,
                Err(err) => {
                    error!("PIPEWIRE: failed to register stream listener: {:?}", err);
                    return;
                }
            };

            let modifier_pod = match Pod::from_bytes(&modifier_bytes) {
                Some(pod) => pod,
                None => {
                    error!("PIPEWIRE: invalid modifier format pod");
                    return;
                }
            };

            let plain_pod = match Pod::from_bytes(&plain_bytes) {
                Some(pod) => pod,
                None => {
                    error!("PIPEWIRE: invalid plain format pod");
                    return;
                }
            };

            let buffer_pod = match Pod::from_bytes(&buffer_bytes) {
                Some(pod) => pod,
                None => {
                    error!("PIPEWIRE: invalid buffer pod");
                    return;
                }
            };

            let mut params = if accepts_dmabuf {
                vec![modifier_pod, plain_pod, buffer_pod]
            } else {
                vec![plain_pod, buffer_pod]
            };

            if let Err(err) = stream.connect(
                pw::spa::utils::Direction::Input,
                Some(node_id),
                pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
                &mut params,
            ) {
                error!("PIPEWIRE: stream connect failed: {:?}", err);
                return;
            }

            info!("PIPEWIRE: stream connected, running main loop");

            main_loop.run();

            info!("PIPEWIRE: loop stopped cleanly");

            drop(portal);
        });

        self.thread_handle = Some(handle);

        Ok(())
    }

    fn stop_capture(&mut self) -> Result<(), ()> {
        if let Some(sender) = self.quit_sender.take() {
            let _ = sender.send(());
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
        callback: Box<dyn Fn(VideoFrame) + Send + Sync>,
        accepts_dmabuf: bool,
    ) -> Result<(), ()> {
        self.start_capture(callback, accepts_dmabuf)
    }

    fn info(&self) -> CaptureInfo {
        self.info.clone()
    }

    fn stop(&mut self) -> Result<(), ()> {
        self.stop_capture()
    }
}
