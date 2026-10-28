mod clock;

pub mod audio;
pub mod buffer;
pub mod capture;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod ipc;
pub mod monitor;
pub mod muxer;
pub mod startup;

use config::{AppState, Config};
use engine::CaptureEngine;

use tracing::{debug, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    if let Some(command) = args.next() {
        return match command.as_str() {
            "save" => {
                let seconds = match args.next() {
                    Some(value) => Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| format!("invalid seconds: {value}"))?,
                    ),
                    None => None,
                };

                match ipc::request_save(seconds).await {
                    Ok((path, warning)) => {
                        if let Some(warning) = warning {
                            eprintln!("warning: {warning}");
                        }
                        println!("{path}");
                        Ok(())
                    }
                    Err(message) => {
                        eprintln!("error: {message}");
                        std::process::exit(1);
                    }
                }
            }
            other => Err(format!("unknown command: {other}").into()),
        };
    }

    let log_path = config::filesystem::log_directory().join("frostclip.log");

    let subscriber = match logging_subscriber::LoggingSubscriber::new().with_file(&log_path) {
        Ok(subscriber) => subscriber,
        Err(_) => logging_subscriber::LoggingSubscriber::new(),
    };

    tracing::subscriber::set_global_default(subscriber)?;

    startup::run().map_err(|e| format!("Startup check failed: {}", e))?;

    info!("Starting capture pipeline");
    debug!("Initializing capture engine...");

    let config = match Config::load() {
        Ok(config) => config,
        Err(_) => {
            let config = Config::default();
            config.save()?;
            config
        }
    };

    let mut state = AppState::load().unwrap_or_default();

    state.update_monitors(monitor::list_monitors());

    for monitor in &state.monitors {
        info!(
            "MONITOR: {} {}x{}@{:.2}Hz at {},{} scale={}",
            monitor.name,
            monitor.width,
            monitor.height,
            monitor.refresh_hz,
            monitor.x,
            monitor.y,
            monitor.scale
        );
    }

    if let Err(err) = state.save() {
        warn!("STATE: failed to save monitors: {}", err);
    }

    let mut engine = CaptureEngine::new(config, state)
        .await
        .map_err(|_| "Failed to create capture engine")?;

    info!("Starting capture...");

    engine.start().map_err(|_| "Failed to start engine")?;

    let listener = ipc::bind()?;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ipc::SaveRequest>(8);
    ipc::serve(listener, tx);

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    info!("Ready, listening on {:?}", ipc::socket_path());

    loop {
        tokio::select! {
            Some(request) = rx.recv() => {
                let result = match request.seconds {
                    Some(seconds) => engine.save_clip_with_duration(seconds),
                    None => engine.save_clip(),
                };

                match &result {
                    Ok(clip) => info!("Clip saved to: {}", clip.path),
                    Err(_) => warn!("Failed to save clip"),
                }

                let _ = request.reply.send(
                    result
                        .map(|clip| (clip.path, clip.warning))
                        .map_err(|err| err.to_string()),
                );
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
        }
    }

    info!("Stopping capture...");

    ipc::cleanup();

    engine.stop().map_err(|_| "Failed to stop engine")?;

    info!("Done!");

    Ok(())
}
