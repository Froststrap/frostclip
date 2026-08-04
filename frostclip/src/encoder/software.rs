use ffmpeg_next as ffmpeg;
use tracing::info;

use crate::{
    encoder::{EncodedPacket, Encoder, VideoInfo, extract_codec_parameters},
    frame::VideoFrame,
};

pub struct SendScaler(pub ffmpeg::software::scaling::Context);

unsafe impl Send for SendScaler {}

pub struct SoftwareEncoder {
    frame_count: u64,
    framerate: u32,

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
        ffmpeg::init().unwrap();

        let codec = ffmpeg::encoder::find_by_name("libx264").expect("libx264 not found");

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);

        let mut encoder = context.encoder().video().unwrap();

        encoder.set_width(output_width);
        encoder.set_height(output_height);
        encoder.set_time_base((1, framerate as i32));
        encoder.set_frame_rate(Some(ffmpeg::Rational::new(framerate as i32, 1)));
        encoder.set_format(ffmpeg::format::Pixel::YUV420P);
        encoder.set_max_b_frames(0);

        //// ABR-style bitrate targeting (x264 doesn't have distinct VBR/CBR modes
        //// like VAAPI. Setting bitrate + maxrate + bufsize gives you VBR behavior).
        ////   bit_rate  = target average
        ////   maxrate   = 1.5x for burst headroom on complex frames
        ////   bufsize   = 2x for VBV constraint window
        ////   gop_size  = framerate (keyframe about every ~1 second)
        let bit_rate = (bitrate_kbps as usize) * 1000;
        let max_rate = bit_rate * 3 / 2;
        let buf_size = bit_rate * 2;

        encoder.set_bit_rate(bit_rate);
        encoder.set_max_bit_rate(max_rate);
        encoder.set_gop(framerate);

        // bufsize isn't exposed on the safe wrapper
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

        let encoder = encoder.open_with(options).unwrap();

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
            .unwrap(),
        );

        info!(
            "SW: initialized input={}x{} output={}x{} fps={}",
            input_width, input_height, output_width, output_height, framerate
        );

        Ok(Self {
            frame_count: 0,
            framerate,

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
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()> {
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
            eprintln!("DMA-BUF unsupported");
            return Err(());
        };

        if format != crate::frame::VideoFormat::Rgba {
            eprintln!("Expected RGBA");
            return Err(());
        }

        if width != self.input_width || height != self.input_height {
            eprintln!(
                "Input resolution changed {}x{} expected {}x{}",
                width, height, self.input_width, self.input_height
            );
            return Err(());
        }

        let mut in_frame = ffmpeg::frame::Video::empty();

        unsafe {
            in_frame.alloc(ffmpeg::format::Pixel::RGBA, width, height);
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

        // Encoder timebase = 1/framerate
        in_frame.set_pts(Some(self.frame_count as i64));

        let mut out_frame = ffmpeg::frame::Video::empty();

        self.scaler.0.run(&in_frame, &mut out_frame).unwrap();

        self.encoder.send_frame(&out_frame).unwrap();

        let mut packets = Vec::new();
        let mut packet = ffmpeg::Packet::empty();

        while self.encoder.receive_packet(&mut packet).is_ok() {
            packets.push(EncodedPacket {
                data: packet.data().unwrap().to_vec(),

                pts: packet.pts().unwrap_or(0),
                dts: packet.dts().unwrap_or(0),
                duration: packet.duration(),

                is_keyframe: packet.is_key(),
            });
        }

        Ok(packets)
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        Ok(Vec::new())
    }

    fn video_info(&self) -> Option<VideoInfo> {
        let extradata =
            unsafe { extract_codec_parameters(self.encoder.as_ptr()).unwrap_or_default() };

        info!("SW: extradata size={}", extradata.len());

        Some(VideoInfo {
            width: self.output_width,
            height: self.output_height,

            time_base_num: 1,
            time_base_den: self.framerate as i32,

            extradata,

            codec: ffmpeg::codec::Id::H264,
            format: ffmpeg::format::Pixel::YUV420P,
        })
    }
}
