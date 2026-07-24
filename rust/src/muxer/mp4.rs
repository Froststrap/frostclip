use anyhow::Result;
use ffmpeg_next as ffmpeg;

use crate::encoder::{EncodedPacket, VideoInfo};

pub struct Mp4Muxer {
    output: ffmpeg::format::context::Output,

    stream_index: usize,
}

impl Mp4Muxer {
    pub fn create(path: &str, info: &VideoInfo) -> Result<Self> {
        ffmpeg::init()?;

        let mut output = ffmpeg::format::output(path)?;
        let mut stream = output.add_stream(info.codec)?;

        {
            let mut params = stream.parameters();

            params.set_codec(info.codec);

            params.set_width(info.width);

            params.set_height(info.height);

            params.set_format(info.format);
            if !info.extradata.is_empty() {
                params.set_extradata(&info.extradata);
            }
        }

        stream.set_time_base(ffmpeg::Rational::new(
            info.time_base_num,
            info.time_base_den,
        ));

        output.write_header()?;

        Ok(Self {
            output,
            stream_index: stream.index(),
        })
    }

    pub fn write(&mut self, packet: &EncodedPacket) -> Result<()> {
        let mut pkt = ffmpeg::Packet::copy(&packet.data);

        pkt.set_stream(self.stream_index);

        pkt.set_pts(Some(packet.timestamp as i64));

        pkt.set_dts(Some(packet.timestamp as i64));

        if packet.is_keyframe {
            pkt.set_flags(ffmpeg::codec::packet::Flags::KEY);
        }

        pkt.write_interleaved(&mut self.output)?;

        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.output.write_trailer()?;

        Ok(())
    }
}
