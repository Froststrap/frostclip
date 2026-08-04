use tracing::{error, info, warn};

pub fn run() -> Result<(), String> {
    log_banner();
    log_platform();
    log_ffmpeg()?;
    log_desktop();
    log_encoders();
    log_paths();
    info!("Ready");
    Ok(())
}

fn log_banner() {
    info!("FrostClip startup");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));
}

fn log_platform() {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    info!("Platform: {} {}", os, arch);
}

fn log_ffmpeg() -> Result<(), String> {
    let packed = unsafe { ffmpeg_sys_next::avcodec_version() };
    let major = (packed >> 16) & 0xFF;
    let minor = (packed >> 8) & 0xFF;
    let micro = packed & 0xFF;

    info!("FFmpeg: libavcodec {}.{}.{}", major, minor, micro);

    /// ffmpeg 8 bindings require libavcodec >= 60
    const MIN_MAJOR: u32 = 60;

    if major < MIN_MAJOR {
        let msg = format!(
            "FrostClip requires libavcodec >= {}.0 (FFmpeg 6.0+). \
             Found {}.{}.{}. Please update FFmpeg.",
            MIN_MAJOR, major, minor, micro
        );
        error!("{}", msg);
        return Err(msg);
    }

    Ok(())
}

fn log_desktop() {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    info!("Desktop: {} ({})", desktop, session_type);
}

fn log_encoders() {
    let nvenc = ffmpeg_next::encoder::find_by_name("h264_nvenc").is_some();
    let vaapi = ffmpeg_next::encoder::find_by_name("h264_vaapi").is_some();
    let x264 = ffmpeg_next::encoder::find_by_name("libx264").is_some();

    info!("Encoders: nvenc={} vaapi={} libx264={}", nvenc, vaapi, x264);

    if !nvenc && !vaapi && !x264 {
        warn!("No H.264 encoders available. FrostClip will not be able to encode.");
    }
}

fn log_paths() {
    let config_path = crate::config::filesystem::user_config_file();
    let state_path = crate::config::filesystem::state_file();

    info!("Config: {}", config_path.display());
    info!("State:  {}", state_path.display());
}
