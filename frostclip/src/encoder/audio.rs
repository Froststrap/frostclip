use ffmpeg_next as ffmpeg;
use tracing::error;

use crate::{
    audio::AudioFrame,
    encoder::{EncodedPacket, extract_codec_parameters, packet::PacketKind},
};

const AAC_FRAME_SIZE: usize = 1024;

#[derive(Clone)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub extradata: Vec<u8>,
}

pub struct AudioEncoder {
    encoder: ffmpeg::codec::encoder::audio::Encoder,
    resampler: ffmpeg::software::resampling::Context,
    sample_rate: u32,
    channels: u16,
    pending_samples: Vec<f32>,
    next_pts: Option<i64>,
    extradata: Vec<u8>,
}

impl AudioEncoder {
    pub fn new(sample_rate: u32, channels: u16, bitrate: u32) -> Result<Self, ()> {
        ffmpeg::init().map_err(|err| {
            error!("AAC: FFmpeg init failed: {}", err);
        })?;

        let layout = match channels {
            1 => ffmpeg::ChannelLayout::MONO,
            2 => ffmpeg::ChannelLayout::STEREO,
            _ => {
                error!("AAC: unsupported channel count {}", channels);
                return Err(());
            }
        };

        let codec = ffmpeg::encoder::find_by_name("aac").ok_or_else(|| {
            error!("AAC: encoder not found");
        })?;

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);
        let mut encoder = context.encoder().audio().map_err(|err| {
            error!("AAC: failed to create encoder: {}", err);
        })?;

        encoder.set_rate(sample_rate as i32);
        encoder.set_channel_layout(layout);
        encoder.set_format(ffmpeg::format::Sample::F32(
            ffmpeg::format::sample::Type::Planar,
        ));
        encoder.set_bit_rate((bitrate as usize) * 1000);
        encoder.set_time_base(ffmpeg::Rational::new(1, sample_rate as i32));
        encoder.set_flags(ffmpeg::codec::Flags::GLOBAL_HEADER);

        let encoder = encoder.open().map_err(|err| {
            error!("AAC: failed to open encoder: {}", err);
        })?;

        let extradata = unsafe { extract_codec_parameters(encoder.as_ptr()).unwrap_or_default() };

        let resampler = ffmpeg::software::resampling::Context::get(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            layout,
            sample_rate,
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Planar),
            layout,
            sample_rate,
        )
        .map_err(|err| {
            error!("AAC: failed to create resampler: {}", err);
        })?;

        Ok(Self {
            encoder,
            resampler,
            sample_rate,
            channels,
            pending_samples: Vec::new(),
            next_pts: None,
            extradata,
        })
    }

    pub fn extradata(&self) -> &[u8] {
        &self.extradata
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn info(&self) -> AudioInfo {
        AudioInfo {
            sample_rate: self.sample_rate,
            channels: self.channels,
            extradata: self.extradata.clone(),
        }
    }

    pub fn submit(&mut self, input: AudioFrame) -> Result<Vec<EncodedPacket>, ()> {
        if input.format.sample_rate != self.sample_rate || input.format.channels != self.channels {
            error!("AAC: input audio format changed");
            return Err(());
        }

        if input.samples.len() % self.channels as usize != 0 {
            error!("AAC: incomplete interleaved sample");
            return Err(());
        }

        if self.next_pts.is_none() {
            self.next_pts = Some(input.pts_us * self.sample_rate as i64 / 1_000_000);
        }

        self.pending_samples.extend(input.samples);

        let mut packets = Vec::new();
        let samples_per_frame = AAC_FRAME_SIZE * self.channels as usize;

        while self.pending_samples.len() >= samples_per_frame {
            let samples: Vec<f32> = self.pending_samples.drain(..samples_per_frame).collect();

            let mut frame = ffmpeg::frame::Audio::new(
                ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
                AAC_FRAME_SIZE,
                match self.channels {
                    1 => ffmpeg::ChannelLayout::MONO,
                    _ => ffmpeg::ChannelLayout::STEREO,
                },
            );

            frame.set_rate(self.sample_rate);

            let bytes = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr() as *const u8,
                    std::mem::size_of_val(samples.as_slice()),
                )
            };

            frame.data_mut(0)[..bytes.len()].copy_from_slice(bytes);

            let pts = self.next_pts.unwrap_or(0);
            frame.set_pts(Some(pts));
            self.next_pts = Some(pts + AAC_FRAME_SIZE as i64);

            let mut converted = ffmpeg::frame::Audio::empty();

            self.resampler.run(&frame, &mut converted).map_err(|err| {
                error!("AAC: resampling failed: {}", err);
            })?;

            converted.set_pts(Some(pts));

            self.encoder.send_frame(&converted).map_err(|err| {
                error!("AAC: failed to submit frame: {}", err);
            })?;

            packets.extend(self.drain());
        }

        Ok(packets)
    }

    fn drain(&mut self) -> Vec<EncodedPacket> {
        let mut packets = Vec::new();

        loop {
            let mut packet = ffmpeg::Packet::empty();

            if self.encoder.receive_packet(&mut packet).is_err() {
                break;
            }

            let pts = packet.pts().unwrap_or(0);
            let dts = packet.dts().unwrap_or(pts);

            packets.push(EncodedPacket {
                kind: PacketKind::Audio,
                data: packet.data().unwrap_or(&[]).to_vec(),
                pts,
                dts,
                duration: packet.duration(),
                is_keyframe: false,
            });
        }

        packets
    }

    pub fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        if !self.pending_samples.is_empty() {
            error!(
                "AAC: {} trailing samples remain; partial-frame padding not implemented",
                self.pending_samples.len()
            );
            self.pending_samples.clear();
        }

        self.encoder.send_eof().map_err(|err| {
            error!("AAC: failed to send EOF: {}", err);
        })?;

        Ok(self.drain())
    }
}
