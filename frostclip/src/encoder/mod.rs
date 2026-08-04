use crate::frame::VideoFrame;
use ffmpeg_sys_next as ffi;

pub mod factory;
pub mod nvenc;
pub mod nvenc_hw;
pub mod software;
pub mod vaapi;
pub mod vaapi_hw;

#[derive(Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,

    pub time_base_num: i32,
    pub time_base_den: i32,

    pub codec: ffmpeg_next::codec::Id,
    pub format: ffmpeg_next::format::Pixel,

    pub extradata: Vec<u8>,
}

pub trait Encoder: Send {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()>;

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()>;

    fn video_info(&self) -> Option<VideoInfo>;
}

pub fn extract_codec_parameters(ctx: *const ffi::AVCodecContext) -> Option<Vec<u8>> {
    unsafe {
        let extradata_size = (*ctx).extradata_size;

        if extradata_size <= 0 {
            return None;
        }

        let data = std::slice::from_raw_parts((*ctx).extradata, extradata_size as usize);

        Some(data.to_vec())
    }
}
