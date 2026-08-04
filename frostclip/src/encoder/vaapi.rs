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

    pub fn new(width: u32, height: u32, framerate: u32, bitrate_kbps: u32) -> Result<Self, ()> {
        ffmpeg::init().unwrap();

        let hw = VaapiHardware::new("/dev/dri/renderD128", width, height).map_err(|_| ())?;

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

        // VBR rate control targeting bitrate_kbps.
        //   maxrate = 1.5x (burst ceiling for complex scenes)
        //   bufsize = 2x (VBV buffer, standard sizing)
        //   bf = 0 (no B-frames, keeps replay latency low)
        //   g = framerate (keyframe about every ~1 second)
        let maxrate = bitrate_kbps * 3 / 2;
        let bufsize = bitrate_kbps * 2;

        let mut options = ffmpeg::Dictionary::new();
        options.set("rc_mode", "VBR");
        options.set("b", &format!("{}k", bitrate_kbps));
        options.set("maxrate", &format!("{}k", maxrate));
        options.set("bufsize", &format!("{}k", bufsize));
        options.set("bf", "0");
        options.set("g", &format!("{}", framerate));

        info!(
            "VAAPI: opening encoder rc=VBR bitrate={}k maxrate={}k bufsize={}k gop={}",
            bitrate_kbps, maxrate, bufsize, framerate
        );

        let encoder = encoder.open_with(options).unwrap();

        info!(
            "VAAPI encoder opened timebase={}/{}",
            encoder.time_base().numerator(),
            encoder.time_base().denominator()
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

        Ok(Self {
            width,
            height,
            frame_count: 0,
            last_packet_pts: None,
            encoder,
            scaler,
            hw,
        })
    }

    pub fn drain(&mut self) -> Vec<EncodedPacket> {
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

            info!(
                "VAAPI output packet pts={} dts={} duration={} key={}",
                pts,
                dts,
                duration,
                packet.is_key()
            );

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

            VideoFrame::LinuxDmaBuf(_) => return Err(()),
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
            if av_hwframe_get_buffer(self.hw.frames_ctx(), hw_frame.as_mut_ptr(), 0) < 0 {
                return Err(());
            }

            if av_hwframe_transfer_data(hw_frame.as_mut_ptr(), nv12.as_ptr(), 0) < 0 {
                return Err(());
            }

            (*hw_frame.as_mut_ptr()).pts = timestamp as i64;
        }

        self.encoder.send_frame(&hw_frame).map_err(|_| ())?;

        self.frame_count += 1;

        Ok(self.drain())
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>, ()> {
        info!("VAAPI flushing encoder");

        self.encoder.send_eof().map_err(|_| ())?;

        let packets = self.drain();

        info!("VAAPI flush returned {} packets", packets.len());

        Ok(packets)
    }

    fn video_info(&self) -> Option<VideoInfo> {
        let extradata =
            unsafe { extract_codec_parameters(self.encoder.as_ptr()).unwrap_or_default() };

        let tb = self.encoder.time_base();

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
