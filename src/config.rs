use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub width: u32,
    pub height: u32,

    pub framerate: u32,
    pub bitrate_kbps: u32,

    pub replay_buffer_seconds: u32,
    pub default_clip_seconds: u32,

    pub output_dir: PathBuf,
}

impl Config {
    pub fn new(
        width: u32,
        height: u32,
        framerate: u32,
        bitrate_kbps: u32,
        replay_buffer_seconds: u32,
        default_clip_seconds: u32,
        output_dir: impl Into<PathBuf>,
    ) -> Result<Self, ()> {
        if default_clip_seconds > replay_buffer_seconds {
            panic!(
                "default_clip_seconds ({}) cannot exceed replay_buffer_seconds ({})",
                default_clip_seconds, replay_buffer_seconds
            );
        }

        Ok(Self {
            width,
            height,

            framerate,
            bitrate_kbps,

            replay_buffer_seconds,
            default_clip_seconds,

            output_dir: output_dir.into(),
        })
    }

    pub fn ensure_output_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.output_dir)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,

            framerate: 60,
            bitrate_kbps: 8000,

            replay_buffer_seconds: 30,
            default_clip_seconds: 30,

            output_dir: PathBuf::from("./clips"),
        }
    }
}
