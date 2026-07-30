use std::{fs, io, path::PathBuf};

const APP_NAME: &str = "frostclip";

pub fn config_directory() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn user_config_file() -> PathBuf {
    config_directory().join("config.toml")
}

pub fn state_file() -> PathBuf {
    config_directory().join("state.toml")
}

pub fn ensure_directory() -> io::Result<()> {
    fs::create_dir_all(config_directory())
}

pub fn read_file(path: &PathBuf) -> io::Result<String> {
    fs::read_to_string(path)
}

pub fn write_file(path: &PathBuf, contents: &str) -> io::Result<()> {
    ensure_directory()?;

    fs::write(path, contents)
}
