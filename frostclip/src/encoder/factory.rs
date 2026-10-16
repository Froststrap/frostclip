use tracing::{info, warn};

use crate::encoder::{
    Encoder, nvenc::NvencEncoder, software::SoftwareEncoder, vaapi::VaapiEncoder,
};

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub input_width: u32,
    pub input_height: u32,

    pub output_width: u32,
    pub output_height: u32,

    pub framerate: u32,
    pub bitrate_kbps: u32,
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

fn try_vaapi(config: EncoderConfig) -> Option<Box<dyn Encoder>> {
    if !VaapiEncoder::probe() {
        return None;
    }

    info!("Using VAAPI encoder");

    match VaapiEncoder::new(
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
