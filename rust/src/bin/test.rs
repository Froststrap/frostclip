use env_logger;
use frostclip::engine::CaptureEngine;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("Initializing capture engine...");

    use frostclip::config::Config;

    let config = Config::new(1366, 768, 60, 5000, "./clips");

    let mut engine = CaptureEngine::new(config)?;

    println!("Starting capture...");
    engine.start()?;

    println!("Capturing for 5 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("Saving clip (5 seconds)...");
    let clip_path = engine.save_clip(5)?;
    println!("✅ Clip saved to: {}", clip_path);

    println!("Stopping capture...");
    engine.stop()?;

    println!("Done!");
    Ok(())
}
