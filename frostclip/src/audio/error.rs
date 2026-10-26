use std::fmt;

#[derive(Debug)]
pub enum AudioError {
    Unsupported,
    AlreadyRunning,
    SourceNotImplemented,
    ThreadExited,
    PipeWire(String),
    Io(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::Unsupported => write!(f, "audio capture not supported on this platform"),
            AudioError::AlreadyRunning => write!(f, "audio capture already running"),
            AudioError::SourceNotImplemented => write!(f, "audio source not implemented yet"),
            AudioError::ThreadExited => write!(f, "audio thread exited early"),
            AudioError::PipeWire(msg) => write!(f, "pipewire error: {}", msg),
            AudioError::Io(msg) => write!(f, "io error: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<std::io::Error> for AudioError {
    fn from(err: std::io::Error) -> Self {
        AudioError::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AudioError>;
