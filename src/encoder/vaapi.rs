use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next::{av_buffer_ref, av_hwframe_get_buffer, av_hwframe_transfer_data};
use tracing::{info, warn};

use crate::encoder::vaapi_hw::VaapiHardware;
use crate::encoder::{EncodedPacket, Encoder, VideoInfo, extract_codec_parameters};
use crate::frame::{VideoFormat, VideoFrame};

pub struct SendScaler(pub ffmpeg::software::scaling::Context);

unsafe impl Send for SendScaler {}

pub struct VaapiEncoder {
    width: u32,
    height: u32,
    frame_count: u64,
    last_packet_pts: Option<i64>,

    encoder: ffmpeg::codec::encoder::video::Encoder,
    scaler: SendScaler,
    hw: VaapiHardware,
}

impl VaapiEncoder {
    pub fn probe() -> bool {
        info!("VAAPI: probing hardware encoder");

        match ffmpeg::encoder::find_by_name("h264_vaapi") {
            Some(_) => {
                info!("VAAPI: h264_vaapi available");
                true
            }

            None => {
                warn!("VAAPI: h264_vaapi not available");
                false
            }
        }
    }

    pub fn new(width: u32, height: u32) -> Result<Self, ()> {
        ffmpeg::init().unwrap();

        let hw = VaapiHardware::new("/dev/dri/renderD128", width, height).unwrap();

        let codec = ffmpeg::encoder::find_by_name("h264_vaapi").expect("h264_vaapi missing");

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);

        let mut encoder = context.encoder().video().unwrap();

        encoder.set_width(width);
        encoder.set_height(height);
        encoder.set_time_base(ffmpeg::Rational::new(1, 1_000_000));

        unsafe {
            (*encoder.as_mut_ptr()).hw_frames_ctx = av_buffer_ref(hw.frames_ctx());
        }

        encoder.set_format(ffmpeg::format::Pixel::VAAPI);

        let mut options = ffmpeg::Dictionary::new();

        options.set("rc_mode", "ICQ");
        options.set("qp", "20");
        options.set("bf", "0");
        options.set("g", "12");

        let encoder = encoder.open_with(options).unwrap();
        info!(
            "VAAPI encoder opened: time_base={}/{} framerate={}/{}",
            encoder.time_base().numerator(),
            encoder.time_base().denominator(),
            encoder.frame_rate().numerator(),
            encoder.frame_rate().denominator()
        );

        let scaler = SendScaler(
            ffmpeg::software::scaling::Context::get(
                ffmpeg::format::Pixel::RGBA,
                width,
                height,
                ffmpeg::format::Pixel::NV12,
                width,
                height,
                ffmpeg::software::scaling::flag::Flags::BILINEAR,
            )
            .unwrap(),
        );

        info!("VAAPI: encoder initialized {}x{}", width, height);

        Ok(Self {
            width,
            height,
            last_packet_pts: None,
            frame_count: 0,
            encoder,
            scaler,
            hw,
        })
    }
}

impl Encoder for VaapiEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>, ()> {
        let (data, width, height, stride, format, timestamp) = match frame {
            VideoFrame::Cpu {
                data,
                width,
                height,
                stride,
                format,
                timestamp,
            } => (data, width, height, stride, format, timestamp),

            VideoFrame::LinuxDmaBuf(_) => {
                return Err(());
            }
        };

        if format != VideoFormat::Rgba {
            return Err(());
        }

        let mut src =
            ffmpeg::util::frame::video::Video::new(ffmpeg::format::Pixel::RGBA, width, height);

        let dst_stride = src.stride(0);

        for y in 0..height as usize {
            let src_start = y * stride as usize;
            let dst_start = y * dst_stride as usize;

            src.data_mut(0)[dst_start..dst_start + width as usize * 4]
                .copy_from_slice(&data[src_start..src_start + width as usize * 4]);
        }

        let mut nv12 =
            ffmpeg::util::frame::video::Video::new(ffmpeg::format::Pixel::NV12, width, height);

        self.scaler.0.run(&src, &mut nv12).unwrap();

        let mut hw_frame = ffmpeg::util::frame::video::Video::empty();

        unsafe {
            let ret = av_hwframe_get_buffer(self.hw.frames_ctx(), hw_frame.as_mut_ptr(), 0);

            if ret < 0 {
                eprintln!("av_hwframe_get_buffer failed {ret}");
                return Err(());
            }

            let ret = av_hwframe_transfer_data(hw_frame.as_mut_ptr(), nv12.as_ptr(), 0);

            if ret < 0 {
                eprintln!("av_hwframe_transfer_data failed {ret}");
                return Err(());
            }

            let pts = timestamp as i64;

            (*hw_frame.as_mut_ptr()).pts = pts;

            info!("VAAPI timestamp={}us converted_pts={}", timestamp, pts);
        }

        info!(
            "VAAPI submit frame {}x{} timestamp={}",
            width, height, timestamp
        );

        self.encoder.send_frame(&hw_frame).unwrap();

        let mut packets = Vec::new();

        loop {
            let mut packet = ffmpeg::Packet::empty();

            match self.encoder.receive_packet(&mut packet) {
                Ok(_) => {
                    info!(
                        "VAAPI packet size={} pts={:?} dts={:?} duration={} keyframe={} encoder_timebase={}/{}",
                        packet.size(),
                        packet.pts(),
                        packet.dts(),
                        packet.duration(),
                        packet.is_key(),
                        self.encoder.time_base().numerator(),
                        self.encoder.time_base().denominator(),
                    );

                    let pts = packet.pts().unwrap_or(0);
                    let dts = packet.dts().unwrap_or(pts);

                    let duration = match self.last_packet_pts {
                        Some(last) => pts - last,
                        None => 0,
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

                Err(_) => {
                    break;
                }
            }
        }

        info!("FRAME COUNT {}", self.frame_count);
        self.frame_count += 1;

        Ok(packets)
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        let mut packets = Vec::new();

        self.encoder.send_eof().unwrap();

        loop {
            let mut packet = ffmpeg::Packet::empty();

            match self.encoder.receive_packet(&mut packet) {
                Ok(_) => {
                    let pts = packet.pts().unwrap_or(0);
                    let dts = packet.dts().unwrap_or(pts);

                    let duration = match self.last_packet_pts {
                        Some(last) => pts - last,
                        None => 0,
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

                Err(_) => {
                    break;
                }
            }
        }

        info!("VAAPI flush complete packets={}", packets.len());

        Ok(packets)
    }

    fn video_info(&self) -> Option<VideoInfo> {
        let extradata =
            unsafe { extract_codec_parameters(self.encoder.as_ptr()).unwrap_or_default() };

        info!("H264 extradata size={}", extradata.len());

        let tb = self.encoder.time_base();

        info!(
            "VIDEO INFO: encoder timebase={}/{}",
            tb.numerator(),
            tb.denominator()
        );

        Some(VideoInfo {
            width: self.width,
            height: self.height,

            time_base_num: tb.numerator(),
            time_base_den: tb.denominator(),

            extradata,

            codec: ffmpeg::codec::Id::H264,
            format: ffmpeg::format::Pixel::YUV420P,
        })
    }
}
