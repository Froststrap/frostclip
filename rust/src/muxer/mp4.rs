use anyhow::Result;
use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next as ffi;

use crate::encoder::{EncodedPacket, VideoInfo};
use crate::muxer::Muxer;

pub struct Mp4Muxer {
    output: ffmpeg::format::context::Output,
    stream_index: usize,
}

impl Mp4Muxer {
    pub fn create(path: &str, info: &VideoInfo) -> Result<Self> {
        ffmpeg::init()?;

        let mut output = ffmpeg::format::output(path)?;

        let stream_index;

        {
            let mut stream = output.add_stream(ffmpeg::codec::Id::H264)?;

            stream.set_time_base(ffmpeg::Rational::new(
                info.time_base_num,
                info.time_base_den,
            ));

            unsafe {
                let params = stream.parameters().as_mut_ptr();

                (*params).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*params).codec_id = ffi::AVCodecID::AV_CODEC_ID_H264;

                (*params).width = info.width as i32;
                (*params).height = info.height as i32;

                (*params).format = ffmpeg::format::Pixel::YUV420P as i32;

                if !info.extradata.is_empty() {
                    let size = info.extradata.len();

                    let data = ffi::av_malloc(size + ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize)
                        as *mut u8;

                    if data.is_null() {
                        anyhow::bail!("failed allocating codec extradata");
                    }

                    std::ptr::copy_nonoverlapping(info.extradata.as_ptr(), data, size);

                    (*params).extradata = data;
                    (*params).extradata_size = size as i32;
                }
            }

            stream_index = stream.index();
        }

        output.write_header()?;

        Ok(Self {
            output,
            stream_index,
        })
    }
}

impl Muxer for Mp4Muxer {
    fn write(&mut self, packet: &EncodedPacket) -> Result<()> {
        let mut pkt = ffmpeg::Packet::copy(&packet.data);

        pkt.set_stream(self.stream_index);

        pkt.set_pts(Some(packet.pts));
        pkt.set_dts(Some(packet.dts));
        pkt.set_duration(packet.duration);

        if packet.is_keyframe {
            pkt.set_flags(ffmpeg::codec::packet::Flags::KEY);
        }

        pkt.write_interleaved(&mut self.output)?;

        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        self.output.write_trailer()?;

        Ok(())
    }
}
