use crate::frame::VideoFrame;
use ffmpeg_sys_next as ffi;

pub mod factory;
pub mod nvenc;
pub mod software;
pub mod vaapi;
pub mod vaapi_hw;

#[derive(Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub is_keyframe: bool,
}

// pub struct CodecParameters {
//     ptr: *mut ffi::AVCodecParameters,
// }

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
    fn submit(&mut self, frame: VideoFrame) -> anyhow::Result<Vec<EncodedPacket>>;

    fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>>;

    fn video_info(&self) -> Option<VideoInfo>;
}

// impl Drop for CodecParameters {
//     fn drop(&mut self) {
//         unsafe {
//             if !self.ptr.is_null() {
//                 ffi::avcodec_parameters_free(&mut self.ptr);
//             }
//         }
//     }
// }

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
