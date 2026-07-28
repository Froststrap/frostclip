use anyhow::Result;
use log::{info, warn};

use crate::encoder::{Encoder, software::SoftwareEncoder, vaapi::VaapiEncoder};

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub input_width: u32,
    pub input_height: u32,

    pub output_width: u32,
    pub output_height: u32,

    pub framerate: u32,
}

pub fn create_encoder(config: EncoderConfig) -> Result<Box<dyn Encoder>> {
    if VaapiEncoder::probe() {
        info!("Using VAAPI encoder");

        match VaapiEncoder::new(config.output_width, config.output_height) {
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
    )?))
}
