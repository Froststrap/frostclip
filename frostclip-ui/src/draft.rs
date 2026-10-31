use std::path::PathBuf;

use config::auto::AutoU32;
use config::user::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Width,
    Height,
    Framerate,
    Bitrate,
    ReplaySeconds,
    ClipSeconds,
    AudioBitrate,
    OutputDir,
}

#[derive(Debug, Clone)]
pub struct Draft {
    base: Config,
    width: String,
    height: String,
    framerate: String,
    bitrate: String,
    replay_seconds: String,
    clip_seconds: String,
    audio_bitrate: String,
    output_dir: String,
    audio_enabled: bool,
}

fn format_auto(value: &AutoU32) -> String {
    match value {
        AutoU32::Auto => String::from("auto"),
        AutoU32::Value(number) => number.to_string(),
    }
}

fn parse_u32(label: &str, text: &str) -> Result<u32, String> {
    match text.trim().parse::<u32>() {
        Ok(0) | Err(_) => Err(format!("invalid {label}: need whole number above 0")),
        Ok(number) => Ok(number),
    }
}

fn parse_auto(label: &str, text: &str) -> Result<AutoU32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return Ok(AutoU32::Auto);
    }
    parse_u32(label, trimmed).map(AutoU32::Value)
}

impl Draft {
    pub fn load() -> (Self, Option<String>) {
        match Config::load() {
            Ok(config) => (Self::from_config(config), None),
            Err(error) => (
                Self::from_config(Config::default()),
                Some(format!("config load failed, showing defaults: {error}")),
            ),
        }
    }

    pub fn from_config(config: Config) -> Self {
        Self {
            width: format_auto(&config.width),
            height: format_auto(&config.height),
            framerate: format_auto(&config.framerate),
            bitrate: config.bitrate_kbps.to_string(),
            replay_seconds: config.replay_buffer_seconds.to_string(),
            clip_seconds: config.default_clip_seconds.to_string(),
            audio_bitrate: config.audio_bitrate_kbps.to_string(),
            output_dir: config.output_dir.display().to_string(),
            audio_enabled: config.audio_enabled,
            base: config,
        }
    }

    pub fn get(&self, key: Key) -> &str {
        match key {
            Key::Width => &self.width,
            Key::Height => &self.height,
            Key::Framerate => &self.framerate,
            Key::Bitrate => &self.bitrate,
            Key::ReplaySeconds => &self.replay_seconds,
            Key::ClipSeconds => &self.clip_seconds,
            Key::AudioBitrate => &self.audio_bitrate,
            Key::OutputDir => &self.output_dir,
        }
    }

    pub fn set(&mut self, key: Key, value: String) {
        match key {
            Key::Width => self.width = value,
            Key::Height => self.height = value,
            Key::Framerate => self.framerate = value,
            Key::Bitrate => self.bitrate = value,
            Key::ReplaySeconds => self.replay_seconds = value,
            Key::ClipSeconds => self.clip_seconds = value,
            Key::AudioBitrate => self.audio_bitrate = value,
            Key::OutputDir => self.output_dir = value,
        }
    }

    pub fn audio_enabled(&self) -> bool {
        self.audio_enabled
    }

    pub fn set_audio_enabled(&mut self, value: bool) {
        self.audio_enabled = value;
    }

    pub fn estimate_megabytes(&self) -> Option<f64> {
        let video = parse_u32("bitrate", &self.bitrate).ok()?;
        let seconds = parse_u32("replay time", &self.replay_seconds).ok()?;
        let audio = if self.audio_enabled {
            parse_u32("audio bitrate", &self.audio_bitrate).ok()?
        } else {
            0
        };
        Some((video + audio) as f64 * seconds as f64 / 8000.0)
    }

    pub fn apply(&self) -> Result<Config, String> {
        let output_dir = self.output_dir.trim();
        if output_dir.is_empty() {
            return Err(String::from("invalid output directory: empty"));
        }

        let mut config = self.base.clone();
        config.width = parse_auto("width", &self.width)?;
        config.height = parse_auto("height", &self.height)?;
        config.framerate = parse_auto("frame rate", &self.framerate)?;
        config.bitrate_kbps = parse_u32("bitrate", &self.bitrate)?;
        config.replay_buffer_seconds = parse_u32("replay time", &self.replay_seconds)?;
        config.default_clip_seconds = parse_u32("clip length", &self.clip_seconds)?;
        config.audio_bitrate_kbps = parse_u32("audio bitrate", &self.audio_bitrate)?;
        config.audio_enabled = self.audio_enabled;
        config.output_dir = PathBuf::from(output_dir);
        Ok(config)
    }
}
