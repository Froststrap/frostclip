use std::path::Path;
use anyhow::{Result, anyhow};

use ffmpeg_next::{
    codec::{self, encoder},
    format::{self, Pixel},
    frame::Video,
    software::scaling,
    Dictionary,
};

pub struct VideoEncoder {
    codec_name: String,
    width: i32,
    height: i32,
    framerate: i32,
    bitrate_kbps: i32,
}

impl VideoEncoder {
    pub fn new(width: i32, height: i32, framerate: i32, bitrate_kbps: i32, codec_name: &str) -> Result<Self> {
        Ok(Self {
            codec_name: codec_name.to_string(),
            width,
            height,
            framerate,
            bitrate_kbps,
        })
    }

    pub fn encode_frames(&self, frames: &[Vec<u8>], output_path: &Path) -> Result<()> {
        ffmpeg_next::init()?;

        let target_width = ((self.width / 4) * 4) as u32;
        let target_height = self.height as u32;

        let encoder_codec = match self.codec_name.as_str() {
            "nvenc" => encoder::find_by_name("h264_nvenc"),
            _ => encoder::find(codec::Id::H264),
        }.ok_or_else(|| anyhow!("Encoder not found for codec: {}", self.codec_name))?;

        let codec_ctx = codec::Context::new_with_codec(encoder_codec);
        let mut encoder_config = codec_ctx.encoder().video()?;

        encoder_config.set_width(target_width);
        encoder_config.set_height(target_height);

        let target_pixel_format = match self.codec_name.as_str() {
            "nvenc" => Pixel::NV12,
            _ => Pixel::YUV420P,
        };
        encoder_config.set_format(target_pixel_format);

        encoder_config.set_frame_rate(Some((self.framerate, 1)));
        encoder_config.set_time_base((1, self.framerate));
        encoder_config.set_bit_rate((self.bitrate_kbps * 1000) as usize);

        let mut output_ctx = format::output(output_path)?;
        let mut stream = output_ctx.add_stream(encoder_codec)?;
        let stream_index = stream.index();

        stream.set_parameters(&encoder_config);

        let mut opts = Dictionary::new();
        if self.codec_name == "nvenc" {
            opts.set("preset", "p1");
            opts.set("tune", "ull");
        } else {
            opts.set("preset", "ultrafast");
            opts.set("tune", "zerolatency");
        }
        let mut encoder = encoder_config.open_with(opts)?;

        output_ctx.write_header()?;
        let stream_time_base = output_ctx.stream(stream_index).unwrap().time_base();

        let mut scaler = scaling::Context::get(
            Pixel::BGRA,
            self.width as u32,
            self.height as u32,
            target_pixel_format,
            target_width,
            target_height,
            scaling::Flags::BILINEAR,
        )?;

        let mut frame_pts: i64 = 0;
        let row_bytes = (self.width * 4) as usize;

        for raw_rgba_frame in frames {
            let mut src_frame = Video::new(Pixel::BGRA, target_width, target_height);

            let stride = src_frame.stride(0);
            let data = src_frame.data_mut(0);
            for (i, row) in raw_rgba_frame.chunks_exact(row_bytes).enumerate() {
                let start = i * stride;
                let end = start + row_bytes;
                if end <= data.len() {
                    data[start..end].copy_from_slice(row);
                }
            }

            let mut dest_frame = Video::new(target_pixel_format, target_width, target_height);
            scaler.run(&src_frame, &mut dest_frame)?;

            dest_frame.set_pts(Some(frame_pts));
            frame_pts += 1;

            encoder.send_frame(&dest_frame)?;
            drain_encoder(&mut encoder, &mut output_ctx, stream_index, stream_time_base)?;
        }

        encoder.send_eof()?;
        drain_encoder(&mut encoder, &mut output_ctx, stream_index, stream_time_base)?;

        output_ctx.write_trailer()?;

        Ok(())
    }
}

fn drain_encoder(
    encoder: &mut ffmpeg_next::encoder::video::Video,
    output_ctx: &mut ffmpeg_next::format::context::Output,
    stream_index: usize,
    stream_time_base: ffmpeg_next::Rational,
) -> Result<()> {
    let mut packet = ffmpeg_next::packet::Packet::empty();
    while encoder.receive_packet(&mut packet).is_ok() {
        packet.set_stream(stream_index);
        packet.rescale_ts(encoder.time_base(), stream_time_base);
        packet.write_interleaved(output_ctx)?;
    }
    Ok(())
}
