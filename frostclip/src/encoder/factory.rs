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

pub fn create_encoder(config: EncoderConfig) -> Result<Box<dyn Encoder>, ()> {
    if NvencEncoder::probe() {
        info!("Using NVENC encoder");

        match NvencEncoder::new(
            config.output_width,
            config.output_height,
            config.framerate,
            config.bitrate_kbps,
        ) {
            Ok(encoder) => {
                return Ok(Box::new(encoder));
            }

            Err(err) => {
                warn!("NVENC init failed {:?}, falling back", err);
            }
        }
    }

    if VaapiEncoder::probe() {
        info!("Using VAAPI encoder");

        match VaapiEncoder::new(
            config.output_width,
            config.output_height,
            config.framerate,
            config.bitrate_kbps,
        ) {
            Ok(encoder) => {
                return Ok(Box::new(encoder));
            }

            Err(err) => {
                warn!("VAAPI init failed {:?}, falling back", err);
            }
        }
    }

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
