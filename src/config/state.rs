use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    pub capture: CaptureState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CaptureState {
    pub session_token: Option<String>,
}

// #[derive(Clone, Debug, Default, Serialize, Deserialize)]
// pub struct LinuxCaptureState {}

impl AppState {
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string_pretty(self)?;

        crate::config::filesystem::write_file(&crate::config::filesystem::state_file(), &contents)?;

        Ok(())
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let contents =
            crate::config::filesystem::read_file(&crate::config::filesystem::state_file())?;

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
}
