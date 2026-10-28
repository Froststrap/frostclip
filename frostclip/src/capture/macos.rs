use crate::capture::{CaptureBackend, CaptureInfo};
use crate::frame::{SendPixelBuffer, VideoFrame};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueAttr};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::*;
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::*;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, mpsc};
use std::time::Instant;

type FrameCallback = Arc<dyn Fn(VideoFrame) + Send + Sync>;

struct Ivars {
    callback: FrameCallback,
    started: Instant,
    accepts_native: bool,
    frames: std::sync::atomic::AtomicU64,
    base_pts_us: std::sync::OnceLock<i64>,
    min_interval_us: i64,
    last_emit_us: AtomicI64,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = Ivars]
    struct OutputHandler;

    unsafe impl NSObjectProtocol for OutputHandler {}

    unsafe impl SCStreamOutput for OutputHandler {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, ty: SCStreamOutputType) {
            if ty != SCStreamOutputType::Screen {
                return;
            }
            self.handle(sample);
        }
    }

    unsafe impl SCStreamDelegate for OutputHandler {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, err: &NSError) {
            tracing::error!("SCK: stream stopped: {:?}", err);
        }
    }
);

impl OutputHandler {
    fn new(iv: Ivars) -> Retained<Self> {
        let this = Self::alloc().set_ivars(iv);
        unsafe { msg_send![super(this), init] }
    }

    fn handle(&self, sample: &CMSampleBuffer) {
        let iv = self.ivars();

        let Some(pixels) = (unsafe { sample.image_buffer() }) else {
            return;
        };

        let width = CVPixelBufferGetWidth(&pixels) as u32;
        let height = CVPixelBufferGetHeight(&pixels) as u32;
        let ts = {
            let pts = unsafe { sample.presentation_time_stamp() };
            if pts.flags.contains(CMTimeFlags::Valid) && pts.timescale != 0 {
                let us = (pts.value as i128 * 1_000_000 / pts.timescale as i128) as i64;
                let base = *iv.base_pts_us.get_or_init(|| us);
                (us - base).max(0) as u64
            } else {
                iv.started.elapsed().as_micros() as u64
            }
        };

        let last = iv.last_emit_us.load(std::sync::atomic::Ordering::Relaxed);
        if last != i64::MIN && (ts as i64 - last) < iv.min_interval_us * 9 / 10 {
            return;
        }
        iv.last_emit_us
            .store(ts as i64, std::sync::atomic::Ordering::Relaxed);

        let n = iv.frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n == 1 || n % 60 == 0 {
            tracing::info!(
                "SCK: frame #{n} {width}x{height} t={:.2}s",
                iv.started.elapsed().as_secs_f64()
            );
        }

        let frame = crate::frame::iosurface::IoSurfaceFrame {
            buffer: SendPixelBuffer(pixels),
            width,
            height,
            timestamp: ts,
        };

        if iv.accepts_native {
            (iv.callback)(VideoFrame::MacIoSurface(frame));
            return;
        }

        match frame.into_cpu() {
            Ok(cpu) => (iv.callback)(cpu),
            Err(err) => tracing::error!("SCK: cpu read failed: {err}"),
        }
    }
}

pub struct Capture {
    display: Retained<SCDisplay>,
    stream: Option<Retained<SCStream>>,
    handler: Option<Retained<OutputHandler>>,
    queue: Option<dispatch2::DispatchRetained<DispatchQueue>>,
    info: CaptureInfo,
}

impl Capture {
    pub async fn new(_state: &mut config::AppState, _target: Option<&str>) -> Result<Self, ()> {
        use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

        if !CGPreflightScreenCaptureAccess() {
            CGRequestScreenCaptureAccess();
            tracing::error!("SCK: no screen recording permission (restart after granting)");
            return Err(());
        }

        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(
            move |content: *mut SCShareableContent, _err: *mut NSError| {
                let _ = tx.send(unsafe { Retained::retain(content) });
            },
        );
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };

        let content = rx.recv().map_err(|_| ())?.ok_or(())?;
        let displays = unsafe { content.displays() };
        let display = displays.firstObject().ok_or(())?;

        let scale = 2u32; // TODO: read the real backing scale
        let info = CaptureInfo {
            width: unsafe { display.width() } as u32 * scale,
            height: unsafe { display.height() } as u32 * scale,
        };

        Ok(Self {
            display,
            stream: None,
            handler: None,
            queue: None,
            info,
        })
    }
}

impl CaptureBackend for Capture {
    fn start(
        &mut self,
        callback: Box<dyn Fn(VideoFrame) + Send + Sync>,
        accepts_dmabuf: bool,
    ) -> Result<(), ()> {
        unsafe {
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &self.display,
                &objc2_foundation::NSArray::new(),
            );

            let cfg = SCStreamConfiguration::new();
            cfg.setWidth(self.info.width as usize);
            cfg.setHeight(self.info.height as usize);
            cfg.setPixelFormat(0x4247_5241); // 'BGRA'
            cfg.setShowsCursor(true);
            cfg.setQueueDepth(5);
            cfg.setMinimumFrameInterval(CMTime {
                value: 1,
                timescale: 60,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });

            const TARGET_FPS: i64 = 60;

            let handler = OutputHandler::new(Ivars {
                callback: callback.into(),
                started: Instant::now(),
                accepts_native: accepts_dmabuf,
                frames: Default::default(),
                base_pts_us: Default::default(),
                min_interval_us: 1_000_000 / TARGET_FPS,
                last_emit_us: AtomicI64::new(i64::MIN),
            });

            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &cfg,
                Some(ProtocolObject::from_ref(&*handler)),
            );

            let queue = DispatchQueue::new("froststrap.capture", DispatchQueueAttr::SERIAL);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*handler),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| tracing::error!("SCK add output: {e:?}"))?;

            let (tx, rx) = mpsc::channel();
            let done = RcBlock::new(move |err: *mut NSError| {
                let _ = tx.send(err.is_null());
            });
            stream.startCaptureWithCompletionHandler(Some(&done));
            if !rx.recv().unwrap_or(false) {
                return Err(());
            }

            self.stream = Some(stream);
            self.handler = Some(handler);
            self.queue = Some(queue);
        }
        Ok(())
    }

    fn info(&self) -> CaptureInfo {
        self.info.clone()
    }

    fn stop(&mut self) -> Result<(), ()> {
        if let Some(stream) = self.stream.take() {
            let (tx, rx) = mpsc::channel();
            let done = RcBlock::new(move |_err: *mut NSError| {
                let _ = tx.send(());
            });
            unsafe { stream.stopCaptureWithCompletionHandler(Some(&done)) };
            let _ = rx.recv();
        }
        self.handler = None;
        self.queue = None;
        Ok(())
    }
}
