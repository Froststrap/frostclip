use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::warn;

use crate::auto::AutoU32;
use crate::state::{AppState, MonitorState};

pub const MAX_AUTO_FRAMERATE: u32 = 60;

const FALLBACK_WIDTH: u32 = 1920;
const FALLBACK_HEIGHT: u32 = 1080;
const FALLBACK_FRAMERATE: u32 = 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_monitor")]
    pub monitor: String,

    pub width: AutoU32,
    pub height: AutoU32,

    pub framerate: AutoU32,
    pub bitrate_kbps: u32,

    pub replay_buffer_seconds: u32,
    pub default_clip_seconds: u32,

    #[serde(default = "default_audio_enabled")]
    pub audio_enabled: bool,

    #[serde(default = "default_audio_bitrate_kbps")]
    pub audio_bitrate_kbps: u32,

    pub output_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub monitor: Option<MonitorState>,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
}

fn default_monitor() -> String {
    "auto".to_string()
}

fn default_audio_enabled() -> bool {
    true
}

fn default_audio_bitrate_kbps() -> u32 {
    192
}

impl Config {
    pub fn ensure_output_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.output_dir)
    }

    pub fn target_monitor(&self, state: &AppState) -> Option<String> {
        if self.monitor.eq_ignore_ascii_case("auto") {
            return None;
        }

        state
            .monitors
            .iter()
            .find(|monitor| monitor.name == self.monitor)
            .map(|monitor| monitor.name.clone())
    }

    pub fn resolve(&self, state: &AppState, capture_width: u32, capture_height: u32) -> Resolved {
        let monitor = self.pick_monitor(state, capture_width, capture_height);

        let width = match self.width {
            AutoU32::Value(value) => value,
            AutoU32::Auto if capture_width > 0 => capture_width,
            AutoU32::Auto => monitor
                .as_ref()
                .map(|monitor| monitor.width)
                .unwrap_or(FALLBACK_WIDTH),
        };

        let height = match self.height {
            AutoU32::Value(value) => value,
            AutoU32::Auto if capture_height > 0 => capture_height,
            AutoU32::Auto => monitor
                .as_ref()
                .map(|monitor| monitor.height)
                .unwrap_or(FALLBACK_HEIGHT),
        };

        let framerate = match self.framerate {
            AutoU32::Value(value) => value,
            AutoU32::Auto => monitor
                .as_ref()
                .filter(|monitor| monitor.refresh_hz >= 1.0)
                .map(|monitor| (monitor.refresh_hz.round() as u32).clamp(1, MAX_AUTO_FRAMERATE))
                .unwrap_or(FALLBACK_FRAMERATE),
        };

        Resolved {
            monitor,
            width,
            height,
            framerate,
        }
    }

    fn pick_monitor(
        &self,
        state: &AppState,
        capture_width: u32,
        capture_height: u32,
    ) -> Option<MonitorState> {
        if !self.monitor.eq_ignore_ascii_case("auto") {
            match state
                .monitors
                .iter()
                .find(|monitor| monitor.name == self.monitor)
            {
                Some(found) => return Some(found.clone()),
                None => warn!("CONFIG: monitor {:?} not found, using auto", self.monitor),
            }
        }

        state
            .monitors
            .iter()
            .find(|monitor| monitor.width == capture_width && monitor.height == capture_height)
            .or_else(|| state.monitors.first())
            .cloned()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string_pretty(self)?;

        crate::filesystem::write_file(&crate::filesystem::user_config_file(), &contents)?;

        Ok(())
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let contents = crate::filesystem::read_file(&crate::filesystem::user_config_file())?;

        Ok(toml::from_str(&contents)?)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            monitor: default_monitor(),
            width: AutoU32::Auto,
            height: AutoU32::Auto,
            framerate: AutoU32::Auto,
            bitrate_kbps: 8000,
            replay_buffer_seconds: 30,
            default_clip_seconds: 30,
            audio_enabled: default_audio_enabled(),
            audio_bitrate_kbps: default_audio_bitrate_kbps(),
            output_dir: PathBuf::from("./clips"),
        }
    }
}
