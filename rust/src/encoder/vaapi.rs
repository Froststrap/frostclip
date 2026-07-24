use anyhow::{Result, anyhow};
use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next::{av_buffer_ref, av_hwframe_get_buffer, av_hwframe_transfer_data};

use log::{info, warn};

use crate::encoder::vaapi_hw::VaapiHardware;
use crate::encoder::{EncodedPacket, Encoder, VideoInfo};
use crate::frame::{VideoFormat, VideoFrame};

pub struct SendScaler(pub ffmpeg::software::scaling::Context);

unsafe impl Send for SendScaler {}

pub struct VaapiEncoder {
    width: u32,
    height: u32,
    frame_count: u64,
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

    pub fn new(width: u32, height: u32) -> Result<Self> {
        ffmpeg::init()?;

        let hw = VaapiHardware::new("/dev/dri/renderD128", width, height)?;

        let codec = ffmpeg::encoder::find_by_name("h264_vaapi")
            .ok_or_else(|| anyhow!("h264_vaapi missing"))?;

        let context = ffmpeg::codec::context::Context::new_with_codec(codec);

        let mut encoder = context.encoder().video()?;

        encoder.set_width(width);
        encoder.set_height(height);

        encoder.set_time_base((1, 60));
        encoder.set_frame_rate(Some(ffmpeg::Rational::new(60, 1)));

        unsafe {
            (*encoder.as_mut_ptr()).hw_frames_ctx = av_buffer_ref(hw.frames_ctx());
        }

        encoder.set_format(ffmpeg::format::Pixel::VAAPI);

        let mut options = ffmpeg::Dictionary::new();

        options.set("rc_mode", "ICQ");
        options.set("qp", "20");

        let encoder = encoder.open_with(options)?;

        let scaler = SendScaler(ffmpeg::software::scaling::Context::get(
            ffmpeg::format::Pixel::BGRA,
            width,
            height,
            ffmpeg::format::Pixel::NV12,
            width,
            height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        )?);

        info!("VAAPI: encoder initialized {}x{}", width, height);

        Ok(Self {
            width,
            height,
            frame_count: 0,
            encoder,
            scaler,
            hw,
        })
    }
}

impl Encoder for VaapiEncoder {
    fn submit(&mut self, frame: VideoFrame) -> Result<Vec<EncodedPacket>> {
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
                return Err(anyhow!("DMA-BUF path not implemented"));
            }
        };

        if format != VideoFormat::Bgra {
            return Err(anyhow!("VAAPI expects BGRA input"));
        }

        let mut src =
            ffmpeg::util::frame::video::Video::new(ffmpeg::format::Pixel::BGRA, width, height);

        let dst_stride = src.stride(0);

        for y in 0..height as usize {
            let src_start = y * stride as usize;
            let dst_start = y * dst_stride as usize;

            src.data_mut(0)[dst_start..dst_start + width as usize * 4]
                .copy_from_slice(&data[src_start..src_start + width as usize * 4]);
        }

        let mut nv12 =
            ffmpeg::util::frame::video::Video::new(ffmpeg::format::Pixel::NV12, width, height);

        self.scaler.0.run(&src, &mut nv12)?;

        let mut hw_frame = ffmpeg::util::frame::video::Video::empty();

        unsafe {
            let ret = av_hwframe_get_buffer(self.hw.frames_ctx(), hw_frame.as_mut_ptr(), 0);

            if ret < 0 {
                return Err(anyhow!("av_hwframe_get_buffer failed {}", ret));
            }

            let ret = av_hwframe_transfer_data(hw_frame.as_mut_ptr(), nv12.as_ptr(), 0);

            if ret < 0 {
                return Err(anyhow!("av_hwframe_transfer_data failed {}", ret));
            }

            (*hw_frame.as_mut_ptr()).pts = timestamp as i64;
        }

        info!(
            "VAAPI submit frame {}x{} timestamp={}",
            width, height, timestamp
        );

        self.encoder.send_frame(&hw_frame)?;

        let mut packets = Vec::new();

        loop {
            let mut packet = ffmpeg::Packet::empty();

            match self.encoder.receive_packet(&mut packet) {
                Ok(_) => {
                    info!(
                        "VAAPI packet size={} keyframe={}",
                        packet.size(),
                        packet.is_key()
                    );

                    packets.push(EncodedPacket {
                        data: packet.data().unwrap_or(&[]).to_vec(),

                        timestamp,

                        is_keyframe: packet.is_key(),
                    });
                }

                Err(_) => {
                    break;
                }
            }
        }

        self.frame_count += 1;

        Ok(packets)
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        let mut packets = Vec::new();

        self.encoder.send_eof()?;

        loop {
            let mut packet = ffmpeg::Packet::empty();

            match self.encoder.receive_packet(&mut packet) {
                Ok(_) => {
                    packets.push(EncodedPacket {
                        data: packet.data().unwrap_or(&[]).to_vec(),
                        timestamp: packet.pts().unwrap_or(0) as u64,
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
        Some(VideoInfo {
            width: self.width,
            height: self.height,

            time_base_num: 1,
            time_base_den: 60,

            extradata: Vec::new(),
            codec: ffmpeg::codec::Id::H264,
            format: ffmpeg::format::Pixel::YUV420P,
        })
    }
}
