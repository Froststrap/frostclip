pub mod buffer;
pub mod capture;
pub mod config;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod muxer;
pub mod portal;
pub mod startup;

use config::{AppState, Config};
use engine::CaptureEngine;

use std::fs;
use std::path::Path;

use tracing::{debug, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::subscriber::set_global_default(logging_subscriber::SimpleSubscriber)?;

    startup::run().map_err(|e| format!("Startup check failed: {}", e))?;

    let runtime = tokio::runtime::Runtime::new()?;

    runtime.block_on(async { run().await })
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting capture pipeline");

    if Path::new("./clips").exists() {
        debug!("Cleaning up existing clips directory...");
        fs::remove_dir_all("./clips")?;
    }

    debug!("Initializing capture engine...");

    let config = Config::new(1366, 768, 60, 3000, 300, 5, "./clips")
        .map_err(|e| format!("Failed to create config: {e:?}"))?;

    let clip_seconds = config.default_clip_seconds;

    let state = AppState::load().unwrap_or_default();

    let mut engine = CaptureEngine::new(config, state)
        .await
        .map_err(|_| "Failed to create capture engine")?;

    info!("Starting capture...");

    engine.start().map_err(|_| "Failed to start engine")?;

    info!("Capturing for {clip_seconds} seconds...");

    tokio::time::sleep(std::time::Duration::from_secs(clip_seconds as u64)).await;

    info!("Saving clip {clip_seconds} seconds...");

    let clip_path = engine.save_clip().map_err(|_| "Failed to save clip")?;

    println!("Clip saved to: {clip_path}");

    info!("Stopping capture...");

    engine.stop().map_err(|_| "Failed to stop engine")?;

    info!("Done!");

    Ok(())
}
