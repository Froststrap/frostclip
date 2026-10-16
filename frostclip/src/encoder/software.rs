use ffmpeg_next as ffmpeg;
use tracing::{error, info};

use crate::{
    encoder::{EncodedPacket, Encoder, VideoInfo, extract_codec_parameters},
    frame::{VideoFormat, VideoFrame},
};

pub struct SendScaler(pub ffmpeg::software::scaling::Context);

unsafe impl Send for SendScaler {}

pub struct SoftwareEncoder {
    last_packet_pts: Option<i64>,

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
        framerate: u32,
        bitrate_kbps: u32,
    ) -> Result<Self, ()> {
        ffmpeg::init().map_err(|err| {
            error!("SW: ffmpeg init failed: {}", err);
        })?;

        let codec = ffmpeg::encoder::find_by_name("libx264").ok_or_else(|| {
            error!("SW: libx264 not found");
        })?;

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);

        let mut encoder = context.encoder().video().map_err(|err| {
            error!("SW: failed to create video encoder: {}", err);
        })?;

        encoder.set_width(output_width);
        encoder.set_height(output_height);
        encoder.set_time_base(ffmpeg::Rational::new(1, 1_000_000));
        encoder.set_frame_rate(Some(ffmpeg::Rational::new(framerate as i32, 1)));
        encoder.set_format(ffmpeg::format::Pixel::YUV420P);
        encoder.set_max_b_frames(0);
        encoder.set_flags(ffmpeg::codec::Flags::GLOBAL_HEADER);

        let bit_rate = (bitrate_kbps as usize) * 1000;
        let max_rate = bit_rate * 3 / 2;
        let buf_size = bit_rate * 2;

        encoder.set_bit_rate(bit_rate);
        encoder.set_max_bit_rate(max_rate);
        encoder.set_gop(framerate);

        unsafe {
            (*encoder.as_mut_ptr()).rc_buffer_size = buf_size as i32;
        }

        let mut options = ffmpeg::Dictionary::new();
        options.set("preset", "veryfast");
        options.set("tune", "zerolatency");

        info!(
            "SW: opening libx264 bitrate={}k maxrate={}k bufsize={}k gop={}",
            bitrate_kbps,
            max_rate / 1000,
            buf_size / 1000,
            framerate
        );

        let encoder = encoder.open_with(options).map_err(|err| {
            error!("SW: failed to open libx264: {}", err);
        })?;

        let scaler = SendScaler(
            ffmpeg::software::scaling::Context::get(
                ffmpeg::format::Pixel::RGBA,
                input_width,
                input_height,
                ffmpeg::format::Pixel::YUV420P,
                output_width,
                output_height,
                ffmpeg::software::scaling::flag::Flags::BILINEAR,
            )
            .map_err(|err| {
                error!("SW: failed to create scaler: {}", err);
            })?,
        );

        info!(
            "SW: initialized input={}x{} output={}x{} fps={}",
            input_width, input_height, output_width, output_height, framerate
        );

        Ok(Self {
            last_packet_pts: None,

            encoder,
            scaler,

            input_width,
            input_height,

            output_width,
            output_height,
        })
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

            let duration = match self.last_packet_pts {
                Some(last) => pts - last,
                None => 1,
            };

            self.last_packet_pts = Some(pts);

            packets.push(EncodedPacket {
                data: packet.data().unwrap_or(&[]).to_vec(),
                pts,
                dts,
                duration,
                is_keyframe: packet.is_key(),
            });
        }

        packets
    }
}

impl Encoder for SoftwareEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()> {
        let VideoFrame::Cpu {
            data,
            width,
            height,
            stride,
            format,
            timestamp,
        } = frame
        else {
            error!("SW: DMA-BUF unsupported");
            return Err(());
        };

        if format != VideoFormat::Rgba {
            error!("SW: expected RGBA, found {:?}", format);
            return Err(());
        }

        if width != self.input_width || height != self.input_height {
            error!(
                "SW: input resolution changed {}x{} expected {}x{}",
                width, height, self.input_width, self.input_height
            );
            return Err(());
        }

        let mut in_frame = ffmpeg::frame::Video::new(ffmpeg::format::Pixel::RGBA, width, height);

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

        let mut out_frame = ffmpeg::frame::Video::empty();

        self.scaler
            .0
            .run(&in_frame, &mut out_frame)
            .map_err(|err| {
                error!("SW: scaling failed: {}", err);
            })?;

        out_frame.set_pts(Some(timestamp as i64));

        self.encoder.send_frame(&out_frame).map_err(|err| {
            error!("SW: failed to submit frame: {}", err);
        })?;

        Ok(self.drain())
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        info!("SW: flushing encoder");

        self.encoder.send_eof().map_err(|err| {
            error!("SW: failed to send EOF: {}", err);
        })?;

        let packets = self.drain();

        info!("SW: flush returned {} packets", packets.len());

        Ok(packets)
    }

    fn video_info(&self) -> Option<VideoInfo> {
        let extradata =
            unsafe { extract_codec_parameters(self.encoder.as_ptr()).unwrap_or_default() };

        let tb = self.encoder.time_base();

        Some(VideoInfo {
            width: self.output_width,
            height: self.output_height,

            time_base_num: tb.numerator(),
            time_base_den: tb.denominator(),

            extradata,

            codec: ffmpeg::codec::Id::H264,
            format: ffmpeg::format::Pixel::YUV420P,
        })
    }
}
