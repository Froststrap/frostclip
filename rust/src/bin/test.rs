use env_logger;
use frostclip::config::Config;
use frostclip::engine::CaptureEngine;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    if Path::new("./clips").exists() {
        println!("Cleaning up existing clips directory...");
        fs::remove_dir_all("./clips")?;
    }

    println!("Initializing capture engine...");

    let config = Config::new(1366, 768, 60, 3000, 30, 15, "./clips")?;

    let clip_seconds = config.default_clip_seconds;

    let mut engine = CaptureEngine::new(config)?;

    println!("Starting capture...");
    engine.start()?;

    println!("Capturing for {} seconds...", clip_seconds);
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("Saving clip {} seconds...", clip_seconds);
    let clip_path = engine.save_clip()?;
    println!("✅ Clip saved to: {}", clip_path);

    println!("Stopping capture...");
    engine.stop()?;

    println!("Done!");
    Ok(())
}
