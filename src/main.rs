pub mod buffer;
pub mod capture;
pub mod config;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod muxer;

use config::Config;
use engine::CaptureEngine;
use std::fs;
use std::path::Path;
use tracing::{debug, info};

fn main() -> Result<(), ()> {
    tracing::subscriber::set_global_default(logging_subscriber::SimpleSubscriber).unwrap();
    tracing::info!("Starting");

    if Path::new("./clips").exists() {
        debug!("Cleaning up existing clips directory...");
        fs::remove_dir_all("./clips").unwrap();
    }

    debug!("Initializing capture engine...");

    let config = Config::new(1366, 768, 60, 3000, 300, 5, "./clips")?;

    let clip_seconds = config.default_clip_seconds;

    let mut engine = CaptureEngine::new(config)?;

    info!("Starting capture...");
    engine.start()?;

    info!("Capturing for {clip_seconds} seconds...");
    std::thread::sleep(std::time::Duration::from_secs(clip_seconds as u64));

    info!("Saving clip {clip_seconds} seconds...");
    let clip_path = engine.save_clip()?;
    println!("Clip saved to: {clip_path}");

    info!("Stopping capture...");
    engine.stop()?;

    info!("Done!");
    Ok(())
}
