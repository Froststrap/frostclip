pub mod buffer;
pub mod capture;
pub mod config;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod muxer;

use config::Config;
use engine::CaptureEngine;
use env_logger;
use std::fs;
use std::path::Path;

fn main() -> Result<(), ()> {
    env_logger::init();

    if Path::new("./clips").exists() {
        println!("Cleaning up existing clips directory...");
        fs::remove_dir_all("./clips").unwrap();
    }

    println!("Initializing capture engine...");

    let config = Config::new(1366, 768, 60, 3000, 300, 5, "./clips")?;

    let clip_seconds = config.default_clip_seconds;

    let mut engine = CaptureEngine::new(config)?;

    println!("Starting capture...");
    engine.start()?;

    println!("Capturing for {} seconds...", clip_seconds);
    std::thread::sleep(std::time::Duration::from_secs(clip_seconds as u64));

    println!("Saving clip {} seconds...", clip_seconds);
    let clip_path = engine.save_clip()?;
    println!("✅ Clip saved to: {}", clip_path);

    println!("Stopping capture...");
    engine.stop()?;

    println!("Done!");
    Ok(())
}
