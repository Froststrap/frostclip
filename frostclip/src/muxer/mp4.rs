use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next as ffi;
use tracing::{debug, error, info};

use crate::encoder::{AudioInfo, EncodedPacket, PacketKind, VideoInfo};
use crate::muxer::Muxer;

struct StreamState {
    index: usize,
    source_time_base: ffmpeg::Rational,
}

pub struct Mp4Muxer {
    output: ffmpeg::format::context::Output,
    video: StreamState,
    audio: Option<StreamState>,
    video_start: Option<i64>,
    audio_start: Option<i64>,
}

fn to_av(rational: ffmpeg::Rational) -> ffi::AVRational {
    ffi::AVRational {
        num: rational.numerator(),
        den: rational.denominator(),
    }
}

fn rescale(value: i64, from: ffmpeg::Rational, to: ffmpeg::Rational) -> i64 {
    unsafe { ffi::av_rescale_q(value, to_av(from), to_av(to)) }
}

unsafe fn set_extradata(params: *mut ffi::AVCodecParameters, data: &[u8]) -> Result<(), ()> {
    if data.is_empty() {
        return Ok(());
    }

    let size = data.len();
    let padding = ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize;

    unsafe {
        let buffer = ffi::av_malloc(size + padding) as *mut u8;

        if buffer.is_null() {
            error!("MUXER: failed allocating codec extradata");
            return Err(());
        }

        std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, size);
        std::ptr::write_bytes(buffer.add(size), 0, padding);

        (*params).extradata = buffer;
        (*params).extradata_size = size as i32;
    }

    Ok(())
}

impl Mp4Muxer {
    pub fn create(path: &str, video: &VideoInfo, audio: Option<&AudioInfo>) -> Result<Self, ()> {
        ffmpeg::init().map_err(|err| {
            error!("MUXER: ffmpeg init failed: {}", err);
        })?;

        let mut output = ffmpeg::format::output(path).map_err(|err| {
            error!("MUXER: failed opening output: {}", err);
        })?;

        let video_time_base = ffmpeg::Rational::new(video.time_base_num, video.time_base_den);

        let video_index = {
            let mut stream = output.add_stream(ffmpeg::codec::Id::H264).map_err(|err| {
                error!("MUXER: failed adding video stream: {}", err);
            })?;

            stream.set_time_base(video_time_base);

            unsafe {
                let params = stream.parameters().as_mut_ptr();

                (*params).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*params).codec_id = ffi::AVCodecID::AV_CODEC_ID_H264;
                (*params).width = video.width as i32;
                (*params).height = video.height as i32;
                (*params).format = ffmpeg::format::Pixel::YUV420P as i32;

                set_extradata(params, &video.extradata)?;
            }

            stream.index()
        };

        let audio_state = match audio {
            Some(info) => {
                let time_base = ffmpeg::Rational::new(1, info.sample_rate as i32);

                let mut stream = output.add_stream(ffmpeg::codec::Id::AAC).map_err(|err| {
                    error!("MUXER: failed adding audio stream: {}", err);
                })?;

                stream.set_time_base(time_base);

                unsafe {
                    let params = stream.parameters().as_mut_ptr();

                    (*params).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
                    (*params).codec_id = ffi::AVCodecID::AV_CODEC_ID_AAC;
                    (*params).format = ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP as i32;
                    (*params).sample_rate = info.sample_rate as i32;
                    (*params).frame_size = 1024;

                    ffi::av_channel_layout_default(&mut (*params).ch_layout, info.channels as i32);

                    set_extradata(params, &info.extradata)?;
                }

                Some(StreamState {
                    index: stream.index(),
                    source_time_base: time_base,
                })
            }
            None => None,
        };

        output.write_header().map_err(|err| {
            error!("MUXER: failed writing header: {}", err);
        })?;

        info!(
            "MUXER: header written video_index={} audio_index={:?}",
            video_index,
            audio_state.as_ref().map(|state| state.index)
        );

        Ok(Self {
            output,
            video: StreamState {
                index: video_index,
                source_time_base: video_time_base,
            },
            audio: audio_state,
            video_start: None,
            audio_start: None,
        })
    }
}

impl Muxer for Mp4Muxer {
    fn write(&mut self, packet: &EncodedPacket) -> Result<(), ()> {
        let (index, source_time_base, start) = match packet.kind {
            PacketKind::Video => {
                let start = *self.video_start.get_or_insert(packet.pts);

                if self.audio_start.is_none() {
                    if let Some(audio) = &self.audio {
                        self.audio_start = Some(rescale(
                            start,
                            self.video.source_time_base,
                            audio.source_time_base,
                        ));
                    }
                }

                (self.video.index, self.video.source_time_base, start)
            }
            PacketKind::Audio => {
                let Some(audio) = &self.audio else {
                    return Ok(());
                };

                let Some(start) = self.audio_start else {
                    return Ok(());
                };

                if packet.pts < start {
                    return Ok(());
                }

                (audio.index, audio.source_time_base, start)
            }
        };

        let stream_time_base = self.output.stream(index).ok_or(())?.time_base();

        let pts = rescale(packet.pts - start, source_time_base, stream_time_base);
        let dts = rescale(packet.dts - start, source_time_base, stream_time_base);
        let duration = rescale(packet.duration, source_time_base, stream_time_base);

        let mut pkt = ffmpeg::Packet::copy(&packet.data);

        pkt.set_stream(index);
        pkt.set_pts(Some(pts));
        pkt.set_dts(Some(dts));
        pkt.set_duration(duration);

        if packet.is_keyframe {
            pkt.set_flags(ffmpeg::codec::packet::Flags::KEY);
        }

        debug!(
            "MUXER WRITE: kind={:?} stream={} pts={} dts={} duration={} key={}",
            packet.kind, index, pts, dts, duration, packet.is_keyframe
        );

        pkt.write_interleaved(&mut self.output).map_err(|err| {
            error!("MUXER: failed writing packet: {}", err);
        })?;

        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), ()> {
        self.output.write_trailer().map_err(|err| {
            error!("MUXER: failed writing trailer: {}", err);
        })?;

        Ok(())
    }
}
