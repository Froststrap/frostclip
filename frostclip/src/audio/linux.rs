use super::{AudioBackend, AudioFormat, AudioFrame, AudioSource};
use anyhow::{Result, anyhow, bail};
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::stream::StreamFlags;
use spa::pod::Pod;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;

struct StreamState {
    info: spa::param::audio::AudioInfoRaw,
    tx: Sender<AudioFrame>,
    start: Instant,
}

pub struct PlatformAudio {
    source: AudioSource,
    quit: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PlatformAudio {
    pub fn new(source: AudioSource) -> Result<Self> {
        if source == AudioSource::Both {
            bail!("AudioSource::Both not implemented yet");
        }
        Ok(Self {
            source,
            quit: None,
            thread: None,
        })
    }
}

impl AudioBackend for PlatformAudio {
    fn start(&mut self) -> Result<Receiver<AudioFrame>> {
        if self.thread.is_some() {
            bail!("audio capture already running");
        }

        let (frame_tx, frame_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let (quit_tx, quit_rx) = pw::channel::channel::<()>();
        let err_tx = ready_tx.clone();
        let source = self.source;

        let handle = std::thread::Builder::new()
            .name("frostclip-audio".into())
            .spawn(move || {
                if let Err(e) = run(source, frame_tx, quit_rx, ready_tx) {
                    let _ = err_tx.send(Err(e));
                }
            })?;

        ready_rx
            .recv()
            .map_err(|_| anyhow!("audio thread exited early"))??;

        self.quit = Some(quit_tx);
        self.thread = Some(handle);
        Ok(frame_rx)
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(quit) = self.quit.take() {
            let _ = quit.send(());
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        Ok(())
    }
}

fn run(
    source: AudioSource,
    frame_tx: Sender<AudioFrame>,
    quit_rx: pw::channel::Receiver<()>,
    ready_tx: Sender<Result<()>>,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let _quit = quit_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::NODE_NAME => "frostclip-audio",
    };
    if source == AudioSource::Desktop {
        props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    }

    let stream = pw::stream::StreamBox::new(&core, "frostclip-audio", props)?;

    let state = StreamState {
        info: spa::param::audio::AudioInfoRaw::new(),
        tx: frame_tx,
        start: Instant::now(),
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .param_changed(|_, state, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != spa::param::format::MediaType::Audio
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            let _ = state.info.parse(param);
        })
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let size = data.chunk().size() as usize;
            let offset = data.chunk().offset() as usize;
            let channels = state.info.channels();
            let rate = state.info.rate();
            if channels == 0 {
                return;
            }
            let Some(bytes) = data.data() else {
                return;
            };
            let end = (offset + size).min(bytes.len());
            if offset >= end {
                return;
            }
            let samples: Vec<f32> = bytes[offset..end]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            let frame = AudioFrame {
                samples,
                pts_us: state.start.elapsed().as_micros() as i64,
                format: AudioFormat {
                    sample_rate: rate,
                    channels: channels as u16,
                },
            };
            let _ = state.tx.send(frame);
        })
        .register()?;

    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(SAMPLE_RATE);
    info.set_channels(CHANNELS);

    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| anyhow!("pod serialize: {e:?}"))?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&bytes).ok_or_else(|| anyhow!("bad format pod"))?];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    let _ = ready_tx.send(Ok(()));
    mainloop.run();
    Ok(())
}
