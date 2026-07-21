use frostclip::capture::CaptureEngine;
use env_logger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("Initializing capture engine...");

    let mut engine = CaptureEngine::new(
        60,
        1920,
        1080,
        5000,
        "/tmp/clips",
        "off",
    )?;

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
