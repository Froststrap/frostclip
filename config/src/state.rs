use serde::{Deserialize, Serialize};
use tracing::info;

use crate::filesystem;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub capture: CaptureState,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monitors: Vec<MonitorState>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CaptureState {
    pub session_token: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MonitorState {
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    pub x: i32,
    pub y: i32,
    pub scale: f64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_token: Option<String>,
}

impl AppState {
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string_pretty(self)?;

        filesystem::write_file(&filesystem::state_file(), &contents)?;
        info!("STATE: saved to {:?}", filesystem::state_file());

        Ok(())
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let contents = filesystem::read_file(&filesystem::state_file())?;

        info!("STATE: loaded from {:?}", filesystem::state_file());

        Ok(toml::from_str(&contents)?)
    }
}

impl AppState {
    pub fn session_token(&self) -> Option<&str> {
        self.capture.session_token.as_deref()
    }

    pub fn set_session_token(&mut self, token: String) {
        self.capture.session_token = Some(token);
    }

    pub fn update_monitors(&mut self, detected: Vec<MonitorState>) {
        if detected.is_empty() {
            return;
        }

        let previous = std::mem::take(&mut self.monitors);

        self.monitors = detected
            .into_iter()
            .map(|mut monitor| {
                if let Some(old) = previous.iter().find(|old| old.name == monitor.name) {
                    monitor.restore_token = old.restore_token.clone();
                }

                monitor
            })
            .collect();
    }

    pub fn monitor(&self, name: &str) -> Option<&MonitorState> {
        self.monitors.iter().find(|monitor| monitor.name == name)
    }

    pub fn monitor_token(&self, name: &str) -> Option<&str> {
        self.monitor(name)?.restore_token.as_deref()
    }

    pub fn set_monitor_token(&mut self, name: &str, token: String) -> bool {
        match self
            .monitors
            .iter_mut()
            .find(|monitor| monitor.name == name)
        {
            Some(monitor) => {
                monitor.restore_token = Some(token);
                true
            }
            None => false,
        }
    }

    pub fn clear_monitor_token(&mut self, name: &str) {
        if let Some(monitor) = self
            .monitors
            .iter_mut()
            .find(|monitor| monitor.name == name)
        {
            monitor.restore_token = None;
        }
    }
}
