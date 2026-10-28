use std::path::PathBuf;

use tokio::net::{UnixListener, UnixStream};

pub struct Listener(UnixListener);

impl Listener {
    pub async fn accept(&self) -> std::io::Result<UnixStream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
}

pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("frostclip.sock")
}

pub fn bind() -> std::io::Result<Listener> {
    let path = socket_path();

    if path.exists() {
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another frostclip instance is running",
            ));
        }
        std::fs::remove_file(&path)?;
    }

    UnixListener::bind(path).map(Listener)
}

pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}

pub async fn connect() -> std::io::Result<UnixStream> {
    UnixStream::connect(socket_path()).await
}
