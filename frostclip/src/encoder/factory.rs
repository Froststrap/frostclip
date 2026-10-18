use ffmpeg_next as ffmpeg;
use tracing::{info, warn};

use super::{
    Encoder,
    platform::{nvenc::NvencEncoder, software::SoftwareEncoder},
};

#[cfg(target_os = "linux")]
use super::platform::linux::VaapiEncoder;

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub input_width: u32,
    pub input_height: u32,

    pub output_width: u32,
    pub output_height: u32,

    pub framerate: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderStatus {
    pub name: &'static str,
    pub supported: bool,
    pub available: bool,
}

fn try_nvenc(config: EncoderConfig) -> Option<Box<dyn Encoder>> {
    if !NvencEncoder::probe() {
        return None;
    }

    info!("Using NVENC encoder");

    match NvencEncoder::new(
        config.output_width,
        config.output_height,
        config.framerate,
        config.bitrate_kbps,
    ) {
        Ok(encoder) => Some(Box::new(encoder)),
        Err(err) => {
            warn!("NVENC init failed {:?}, falling back", err);
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn try_vaapi(config: EncoderConfig) -> Option<Box<dyn Encoder>> {
    if !VaapiEncoder::probe() {
        return None;
    }

    info!("Using VAAPI encoder");

    match VaapiEncoder::new(
        config.input_width,
        config.input_height,
        config.output_width,
        config.output_height,
        config.framerate,
        config.bitrate_kbps,
    ) {
        Ok(encoder) => Some(Box::new(encoder)),
        Err(err) => {
            warn!("VAAPI init failed {:?}, falling back", err);
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn try_vaapi(_config: EncoderConfig) -> Option<Box<dyn Encoder>> {
    None
}

fn software(config: EncoderConfig) -> Result<Box<dyn Encoder>, ()> {
    info!("Using software encoder (libx264)");

    Ok(Box::new(SoftwareEncoder::new(
        config.input_width,
        config.input_height,
        config.output_width,
        config.output_height,
        config.framerate,
        config.bitrate_kbps,
    )?))
}

pub fn encoder_statuses() -> Vec<EncoderStatus> {
    vec![
        EncoderStatus {
            name: "nvenc",
            supported: true,
            available: ffmpeg::encoder::find_by_name("h264_nvenc").is_some(),
        },
        EncoderStatus {
            name: "vaapi",
            supported: cfg!(target_os = "linux"),
            available: cfg!(target_os = "linux")
                && ffmpeg::encoder::find_by_name("h264_vaapi").is_some(),
        },
        EncoderStatus {
            name: "libx264",
            supported: true,
            available: ffmpeg::encoder::find_by_name("libx264").is_some(),
        },
    ]
}

pub fn create_encoder(config: EncoderConfig) -> Result<Box<dyn Encoder>, ()> {
    match std::env::var("FROSTCLIP_ENCODER").ok().as_deref() {
        Some("software") => return software(config),
        Some("vaapi") => {
            return try_vaapi(config).ok_or_else(|| {
                warn!("FROSTCLIP_ENCODER=vaapi requested but unavailable");
            });
        }
        Some("nvenc") => {
            return try_nvenc(config).ok_or_else(|| {
                warn!("FROSTCLIP_ENCODER=nvenc requested but unavailable");
            });
        }
        Some(other) => warn!("FROSTCLIP_ENCODER={} unknown, using auto", other),
        None => {}
    }

    if let Some(encoder) = try_nvenc(config) {
        return Ok(encoder);
    }

    if let Some(encoder) = try_vaapi(config) {
        return Ok(encoder);
    }

    software(config)
}
