use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};

use objc2_core_foundation::{CFBoolean, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMSampleBuffer, CMTime, CMTimeFlags, CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
    kCMTimeInvalid, kCMVideoCodecType_H264,
};
use objc2_core_video::CVPixelBuffer;
use objc2_video_toolbox::*;
use tracing::{error, info};

use crate::encoder::{EncodedPacket, Encoder, VideoInfo};
use crate::frame::{SendPixelBuffer, VideoFrame};

const TIMESCALE: i32 = 1_000_000;
const START_CODE: [u8; 4] = [0, 0, 0, 1];

#[derive(Default)]
struct Shared {
    packets: Mutex<Vec<EncodedPacket>>,
    extradata: Mutex<Option<Vec<u8>>>,
    last_pts: Mutex<Option<i64>>,
}

pub struct VideoToolboxEncoder {
    session: CFRetained<VTCompressionSession>,
    shared: Arc<Shared>,
    refcon: *const Shared,
    width: u32,
    height: u32,
    frame_us: i64,
    last: Option<SendPixelBuffer>,
    last_pts: i64,
    first: bool,
}

unsafe impl Send for VideoToolboxEncoder {}

fn set_prop(session: &VTCompressionSession, key: &CFString, value: &CFType, name: &str) {
    let status = unsafe { VTSessionSetProperty(session.as_ref(), key, Some(value)) };
    if status != 0 {
        error!("VT: setting {name} failed ({status})");
    }
}

impl VideoToolboxEncoder {
    pub fn new(width: u32, height: u32, framerate: u32, bitrate_kbps: u32) -> Result<Self, ()> {
        let shared = Arc::new(Shared::default());
        let refcon = Arc::into_raw(shared.clone());

        let mut raw: *mut VTCompressionSession = ptr::null_mut();
        let status = unsafe {
            VTCompressionSession::create(
                None,
                width as i32,
                height as i32,
                kCMVideoCodecType_H264,
                None,
                None,
                None,
                Some(output_callback),
                refcon as *mut c_void,
                NonNull::from(&mut raw),
            )
        };

        let Some(raw) = NonNull::new(raw).filter(|_| status == 0) else {
            error!("VT: VTCompressionSessionCreate failed ({status})");
            unsafe { drop(Arc::from_raw(refcon)) };
            return Err(());
        };
        let session = unsafe { CFRetained::from_raw(raw) };

        unsafe {
            set_prop(
                &session,
                kVTCompressionPropertyKey_RealTime,
                CFBoolean::new(true).as_ref(),
                "RealTime",
            );
            set_prop(
                &session,
                kVTCompressionPropertyKey_AllowFrameReordering,
                CFBoolean::new(false).as_ref(),
                "AllowFrameReordering",
            );
            set_prop(
                &session,
                kVTCompressionPropertyKey_ProfileLevel,
                kVTProfileLevel_H264_High_AutoLevel.as_ref(),
                "ProfileLevel",
            );
            set_prop(
                &session,
                kVTCompressionPropertyKey_AverageBitRate,
                CFNumber::new_i32((bitrate_kbps * 1000) as i32).as_ref(),
                "AverageBitRate",
            );
            set_prop(
                &session,
                kVTCompressionPropertyKey_ExpectedFrameRate,
                CFNumber::new_i32(framerate as i32).as_ref(),
                "ExpectedFrameRate",
            );
            set_prop(
                &session,
                kVTCompressionPropertyKey_MaxKeyFrameInterval,
                CFNumber::new_i32(framerate as i32).as_ref(),
                "MaxKeyFrameInterval",
            );
            session.prepare_to_encode_frames();
        }

        info!("VT: initialized {width}x{height} fps={framerate} bitrate={bitrate_kbps}k");

        Ok(Self {
            session,
            shared,
            refcon,
            width,
            height,
            frame_us: 1_000_000 / framerate.max(1) as i64,
            last: None,
            last_pts: -1,
            first: true,
        })
    }

    fn encode(&mut self, buffer: &CVPixelBuffer, ts_us: i64) -> Result<(), ()> {
        let ts = ts_us.max(self.last_pts + 1);

        let pts = CMTime {
            value: ts,
            timescale: TIMESCALE,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        let dur = CMTime {
            value: self.frame_us,
            timescale: TIMESCALE,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };

        let status = unsafe {
            self.session
                .encode_frame(buffer, pts, dur, None, ptr::null_mut(), ptr::null_mut())
        };
        if status != 0 {
            error!("VT: encode_frame failed ({status})");
            return Err(());
        }
        self.last_pts = ts;

        if self.first {
            self.first = false;
            unsafe { self.session.complete_frames(kCMTimeInvalid) };
        }
        Ok(())
    }

    fn take_packets(&self) -> Vec<EncodedPacket> {
        std::mem::take(&mut *self.shared.packets.lock().unwrap())
    }
}

impl Encoder for VideoToolboxEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()> {
        let VideoFrame::MacIoSurface(f) = frame else {
            error!("VT: only MacIoSurface frames are supported");
            return Err(());
        };

        self.encode(&f.buffer.0, f.timestamp as i64)?;
        self.last = Some(f.buffer);

        Ok(self.take_packets())
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        info!("VT: flushing encoder");
        unsafe { self.session.complete_frames(kCMTimeInvalid) };
        let packets = self.take_packets();
        info!("VT: flush returned {} packets", packets.len());
        Ok(packets)
    }

    fn video_info(&self) -> Option<VideoInfo> {
        let extradata = self
            .shared
            .extradata
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();

        Some(VideoInfo {
            width: self.width,
            height: self.height,
            time_base_num: 1,
            time_base_den: TIMESCALE,
            extradata,
            codec: ffmpeg_next::codec::Id::H264,
            format: ffmpeg_next::format::Pixel::YUV420P,
        })
    }

    fn accepts_dmabuf(&self) -> bool {
        true
    }

    fn repeat_last(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        let Some(buffer) = self.last.as_ref().map(|b| b.0.clone()) else {
            return Ok(Vec::new());
        };
        self.encode(&buffer, self.last_pts + self.frame_us)?;
        Ok(self.take_packets())
    }
}

impl Drop for VideoToolboxEncoder {
    fn drop(&mut self) {
        unsafe {
            self.session.complete_frames(kCMTimeInvalid);
            self.session.invalidate();
            drop(Arc::from_raw(self.refcon));
        }
    }
}

unsafe extern "C-unwind" fn output_callback(
    refcon: *mut c_void,
    _source_frame_refcon: *mut c_void,
    status: i32,
    _flags: VTEncodeInfoFlags,
    sample: *mut CMSampleBuffer,
) {
    if status != 0 || refcon.is_null() {
        return;
    }
    let Some(sample) = (unsafe { sample.as_ref() }) else {
        return;
    };
    let shared = unsafe { &*(refcon as *const Shared) };

    {
        let mut extra = shared.extradata.lock().unwrap();
        if extra.is_none() {
            *extra = unsafe { annexb_parameter_sets(sample) };
        }
    }

    let Some(avcc) = (unsafe { sample_bytes(sample) }) else {
        return;
    };
    let mut data = Vec::with_capacity(avcc.len() + 16);
    let is_keyframe = avcc_to_annexb(&avcc, &mut data);

    let pts = cm_to_us(unsafe { sample.presentation_time_stamp() });

    let mut last = shared.last_pts.lock().unwrap();
    let duration = last.map_or(1, |l| pts - l);
    *last = Some(pts);

    shared.packets.lock().unwrap().push(EncodedPacket {
        data,
        pts,
        dts: pts,
        duration,
        is_keyframe,
    });
}

fn cm_to_us(t: CMTime) -> i64 {
    if t.timescale == 0 {
        return 0;
    }
    (t.value as i128 * 1_000_000 / t.timescale as i128) as i64
}

unsafe fn sample_bytes(sample: &CMSampleBuffer) -> Option<Vec<u8>> {
    let block = unsafe { sample.data_buffer() }?;
    let len = unsafe { block.data_length() };
    let mut bytes = vec![0u8; len];
    let st = unsafe { block.copy_data_bytes(0, len, NonNull::new(bytes.as_mut_ptr().cast())?) };
    (st == 0).then_some(bytes)
}

unsafe fn annexb_parameter_sets(sample: &CMSampleBuffer) -> Option<Vec<u8>> {
    let desc = unsafe { sample.format_description() }?;
    let mut out = Vec::new();
    let mut index = 0usize;
    loop {
        let mut ptr: *const u8 = ptr::null();
        let mut size = 0usize;
        let mut count = 0usize;
        let mut nal_header_len = 0i32;
        let st = unsafe {
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                &desc,
                index,
                &mut ptr,
                &mut size,
                &mut count,
                &mut nal_header_len,
            )
        };
        if st != 0 || ptr.is_null() {
            return None;
        }
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(ptr, size) });
        index += 1;
        if index >= count {
            return Some(out);
        }
    }
}

fn avcc_to_annexb(avcc: &[u8], out: &mut Vec<u8>) -> bool {
    let mut i = 0;
    let mut idr = false;
    while i + 4 <= avcc.len() {
        let len = u32::from_be_bytes(avcc[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if len == 0 || i + len > avcc.len() {
            break;
        }
        if avcc[i] & 0x1f == 5 {
            idr = true;
        }
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(&avcc[i..i + len]);
        i += len;
    }
    idr
}
