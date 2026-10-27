mod clock;

pub mod audio;
pub mod buffer;
pub mod capture;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod muxer;
pub mod startup;

use config::{AppState, Config};
use engine::CaptureEngine;

use std::fs;
use std::path::Path;

use tracing::{debug, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_path = config::filesystem::log_directory().join("frostclip.log");

    let subscriber = match logging_subscriber::LoggingSubscriber::new().with_file(&log_path) {
        Ok(subscriber) => subscriber,
        Err(_) => logging_subscriber::LoggingSubscriber::new(),
    };

    tracing::subscriber::set_global_default(subscriber)?;

    startup::run().map_err(|e| format!("Startup check failed: {}", e))?;

    info!("Starting capture pipeline");

    if Path::new("./clips").exists() {
        debug!("Cleaning up existing clips directory...");
        fs::remove_dir_all("./clips")?;
    }

    debug!("Initializing capture engine...");

    let config = match Config::load() {
        Ok(config) => config,
        Err(_) => {
            let config = Config::default();
            config.save()?;
            config
        }
    };

    let clip_seconds = config.default_clip_seconds;

    let state = AppState::load().unwrap_or_default();

    let mut engine = CaptureEngine::new(config, state)
        .await
        .map_err(|_| "Failed to create capture engine")?;

    info!("Starting capture...");

    engine.start().map_err(|_| "Failed to start engine")?;

    info!("Capturing for {} seconds...", clip_seconds + 3);

    tokio::time::sleep(std::time::Duration::from_secs(clip_seconds as u64 + 3)).await;

    info!("Saving clip {clip_seconds} seconds...");

    let clip_path = engine.save_clip().map_err(|_| "Failed to save clip")?;

    println!("Clip saved to: {clip_path}");

    info!("Stopping capture...");

    engine.stop().map_err(|_| "Failed to stop engine")?;

    info!("Done!");

    Ok(())
}
