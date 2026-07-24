use anyhow::{Result, anyhow};
use ffmpeg_next as ffmpeg;

use crate::{
    encoder::{EncodedPacket, Encoder, VideoInfo},
    frame::VideoFrame,
};

pub struct SendScaler(pub ffmpeg::software::scaling::Context);

unsafe impl Send for SendScaler {}

pub struct SoftwareEncoder {
    frame_count: u64,

    encoder: ffmpeg::codec::encoder::video::Encoder,

    scaler: SendScaler,

    input_width: u32,
    input_height: u32,

    output_width: u32,
    output_height: u32,
}

impl SoftwareEncoder {
    pub fn new(
        input_width: u32,
        input_height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<Self> {
        ffmpeg::init()?;

        let codec =
            ffmpeg::encoder::find_by_name("libx264").ok_or_else(|| anyhow!("libx264 not found"))?;

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);

        let mut encoder = context.encoder().video()?;

        encoder.set_width(output_width);
        encoder.set_height(output_height);
        encoder.set_time_base((1, 60));
        encoder.set_frame_rate(Some(ffmpeg::Rational::new(60, 1)));
        encoder.set_format(ffmpeg::format::Pixel::YUV420P);
        encoder.set_max_b_frames(0);

        let mut options = ffmpeg::Dictionary::new();
        options.set("preset", "veryfast");
        options.set("tune", "zerolatency");

        let encoder = encoder.open_with(options)?;

        let scaler = SendScaler(ffmpeg::software::scaling::Context::get(
            ffmpeg::format::Pixel::BGRA,
            input_width,
            input_height,
            ffmpeg::format::Pixel::YUV420P,
            output_width,
            output_height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        )?);

        println!(
            "H264 initialized input={}x{} output={}x{}",
            input_width, input_height, output_width, output_height
        );

        Ok(Self {
            frame_count: 0,
            encoder,
            scaler,

            input_width,
            input_height,

            output_width,
            output_height,
        })
    }
}

impl Encoder for SoftwareEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>> {
        self.frame_count += 1;

        let VideoFrame::Cpu {
            data,
            width,
            height,
            stride,
            format,
            ..
        } = frame
        else {
            return Err(anyhow!("DMA-BUF unsupported"));
        };

        if format != crate::frame::VideoFormat::Bgra {
            return Err(anyhow!("Expected BGRA"));
        }

        if width != self.input_width || height != self.input_height {
            return Err(anyhow!(
                "Input resolution changed {}x{} expected {}x{}",
                width,
                height,
                self.input_width,
                self.input_height
            ));
        }

        let mut in_frame = ffmpeg::frame::Video::empty();

        unsafe {
            in_frame.alloc(ffmpeg::format::Pixel::BGRA, width, height);
        }

        let dst_stride = in_frame.stride(0);
        let src_stride = stride as usize;
        let line_bytes = width as usize * 4;

        {
            let plane = in_frame.data_mut(0);

            for y in 0..height as usize {
                let src = y * src_stride;
                let dst = y * dst_stride;

                plane[dst..dst + line_bytes].copy_from_slice(&data[src..src + line_bytes]);
            }
        }

        // Encoder timebase = 1/60
        in_frame.set_pts(Some(self.frame_count as i64));

        let mut out_frame = ffmpeg::frame::Video::empty();

        self.scaler.0.run(&in_frame, &mut out_frame)?;

        self.encoder.send_frame(&out_frame)?;

        let mut packets = Vec::new();
        let mut packet = ffmpeg::Packet::empty();

        while self.encoder.receive_packet(&mut packet).is_ok() {
            packets.push(EncodedPacket {
                data: packet.data().unwrap().to_vec(),
                is_keyframe: packet.is_key(),
                timestamp: self.frame_count,
            });
        }

        Ok(packets)
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }

    fn video_info(&self) -> Option<VideoInfo> {
        Some(VideoInfo {
            width: self.output_width,
            height: self.output_height,
            time_base_num: 1,
            time_base_den: 60,

            extradata: Vec::new(),
            codec: ffmpeg::codec::Id::H264,
            format: ffmpeg::format::Pixel::YUV420P,
        })
    }
}
