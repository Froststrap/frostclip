pub mod audio;
mod packet;
mod platform;
mod traits;
mod video_info;

pub mod factory;

pub use audio::AudioInfo;
pub use factory::{EncoderConfig, EncoderStatus, create_encoder, encoder_statuses};
pub use packet::{EncodedPacket, PacketKind};
pub use traits::Encoder;
pub use video_info::VideoInfo;

use ffmpeg_sys_next as ffi;

pub(crate) fn extract_codec_parameters(ctx: *const ffi::AVCodecContext) -> Option<Vec<u8>> {
    unsafe {
        let extradata_size = (*ctx).extradata_size;

        if extradata_size <= 0 {
            return None;
        }

        let data = std::slice::from_raw_parts((*ctx).extradata, extradata_size as usize);

        Some(data.to_vec())
    }
}
