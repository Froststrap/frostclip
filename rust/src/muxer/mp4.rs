use anyhow::Result;
use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next as ffi;
use log::info;

use crate::encoder::{EncodedPacket, VideoInfo};
use crate::muxer::Muxer;

pub struct Mp4Muxer {
    output: ffmpeg::format::context::Output,
    stream_index: usize,
    start_pts: Option<i64>,
    encoder_time_base: ffmpeg::Rational,
}

impl Mp4Muxer {
    pub fn create(path: &str, info: &VideoInfo) -> Result<Self> {
        ffmpeg::init()?;

        let mut output = ffmpeg::format::output(path)?;

        let stream_index;

        {
            let mut stream = output.add_stream(ffmpeg::codec::Id::H264)?;

            let stream_time_base = ffmpeg::Rational::new(info.time_base_num, info.time_base_den);

            info!(
                "MUXER: video timebase={}/{}",
                stream_time_base.numerator(),
                stream_time_base.denominator()
            );
            stream.set_time_base(stream_time_base);

            info!(
                "MUXER: actual stream timebase={}/{}",
                stream.time_base().numerator(),
                stream.time_base().denominator()
            );

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

        info!(
            "MUXER: writing header stream_index={} timebase={}/{}",
            stream_index,
            output.stream(stream_index).unwrap().time_base().numerator(),
            output
                .stream(stream_index)
                .unwrap()
                .time_base()
                .denominator()
        );

        output.write_header()?;

        Ok(Self {
            output,
            stream_index,
            start_pts: None,
            encoder_time_base: ffmpeg::Rational::new(info.time_base_num, info.time_base_den),
        })
    }
}

impl Muxer for Mp4Muxer {
    fn write(&mut self, packet: &EncodedPacket) -> Result<()> {
        let mut pkt = ffmpeg::Packet::copy(&packet.data);

        pkt.set_stream(self.stream_index);

        let stream = self.output.stream(self.stream_index).unwrap();

        let muxer_tb = stream.time_base();

        let start_pts = self.start_pts.get_or_insert(packet.pts);

        let adjusted_pts = packet.pts - *start_pts;
        let adjusted_dts = packet.dts - *start_pts;

        let muxer_tb = stream.time_base();

        let pts = unsafe {
            ffi::av_rescale_q(
                adjusted_pts,
                ffi::AVRational {
                    num: self.encoder_time_base.numerator(),
                    den: self.encoder_time_base.denominator(),
                },
                ffi::AVRational {
                    num: muxer_tb.numerator(),
                    den: muxer_tb.denominator(),
                },
            )
        };

        let dts = unsafe {
            ffi::av_rescale_q(
                adjusted_dts,
                ffi::AVRational {
                    num: self.encoder_time_base.numerator(),
                    den: self.encoder_time_base.denominator(),
                },
                ffi::AVRational {
                    num: muxer_tb.numerator(),
                    den: muxer_tb.denominator(),
                },
            )
        };

        let duration = unsafe {
            ffi::av_rescale_q(
                packet.duration,
                ffi::AVRational {
                    num: self.encoder_time_base.numerator(),
                    den: self.encoder_time_base.denominator(),
                },
                ffi::AVRational {
                    num: muxer_tb.numerator(),
                    den: muxer_tb.denominator(),
                },
            )
        };

        info!(
            "RESCALE: {}/{} -> {}/{} | pts {}->{} dts {}->{} duration {}->{}",
            self.encoder_time_base.numerator(),
            self.encoder_time_base.denominator(),
            muxer_tb.numerator(),
            muxer_tb.denominator(),
            packet.pts,
            pts,
            packet.dts,
            dts,
            packet.duration,
            duration
        );

        pkt.set_pts(Some(pts));
        pkt.set_dts(Some(dts));
        pkt.set_duration(duration);

        if packet.is_keyframe {
            pkt.set_flags(ffmpeg::codec::packet::Flags::KEY);
        }

        info!(
            "MUXER WRITE: pts={} dts={} duration={} stream={} stream_tb={}/{}",
            packet.pts,
            packet.dts,
            packet.duration,
            self.stream_index,
            self.output
                .stream(self.stream_index)
                .unwrap()
                .time_base()
                .numerator(),
            self.output
                .stream(self.stream_index)
                .unwrap()
                .time_base()
                .denominator()
        );

        pkt.write_interleaved(&mut self.output)?;
        if packet.is_keyframe {
            info!(
                "KEYFRAME DATA: {:02x?}",
                &packet.data[..std::cmp::min(packet.data.len(), 32)]
            );
        }

        info!(
            "FINAL PACKET: pts={} dts={} duration={} key={}",
            pts, dts, duration, packet.is_keyframe
        );

        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        self.output.write_trailer()?;

        Ok(())
    }
}
