use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

pub struct SaveRequest {
    pub seconds: Option<usize>,
    pub reply: oneshot::Sender<Result<(String, Option<String>), String>>,
}

pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("frostclip.sock")
}

pub fn bind() -> std::io::Result<UnixListener> {
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

    UnixListener::bind(path)
}

pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}

pub fn serve(listener: UnixListener, tx: mpsc::Sender<SaveRequest>) {
    tokio::spawn(async move {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(err) => {
                    warn!("IPC: accept failed: {}", err);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };

            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(err) = handle(stream, tx).await {
                    debug!("IPC: connection error: {}", err);
                }
            });
        }
    });
}

async fn handle(stream: UnixStream, tx: mpsc::Sender<SaveRequest>) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();

    let Some(line) = lines.next_line().await? else {
        return Ok(());
    };

    let mut parts = line.split_whitespace();

    let response = match parts.next() {
        Some("save") => {
            let seconds = match parts.next() {
                Some(value) => match value.parse::<usize>() {
                    Ok(seconds) if seconds > 0 => Some(seconds),
                    _ => {
                        write.write_all(b"err invalid seconds\n").await?;
                        return Ok(());
                    }
                },
                None => None,
            };

            let (reply, receiver) = oneshot::channel();

            if tx.send(SaveRequest { seconds, reply }).await.is_err() {
                "err engine stopped".to_string()
            } else {
                match receiver.await {
                    Ok(Ok((path, Some(warning)))) => format!("ok {path}\nwarn {warning}"),
                    Ok(Ok((path, None))) => format!("ok {path}"),
                    Ok(Err(message)) => format!("err {message}"),
                    Err(_) => "err engine stopped".to_string(),
                }
            }
        }
        _ => "err unknown command".to_string(),
    };

    write.write_all(response.as_bytes()).await?;
    write.write_all(b"\n").await?;

    Ok(())
}

pub async fn request_save(seconds: Option<usize>) -> Result<(String, Option<String>), String> {
    let mut stream = UnixStream::connect(socket_path())
        .await
        .map_err(|err| format!("cannot reach daemon: {err}"))?;

    let command = match seconds {
        Some(seconds) => format!("save {seconds}\n"),
        None => "save\n".to_string(),
    };

    stream
        .write_all(command.as_bytes())
        .await
        .map_err(|err| err.to_string())?;

    let mut lines = BufReader::new(stream).lines();

    let line = lines
        .next_line()
        .await
        .map_err(|err| err.to_string())?
        .ok_or("daemon closed connection")?;

    let path = match line.strip_prefix("ok ") {
        Some(path) => path.to_string(),
        None => return Err(line.strip_prefix("err ").unwrap_or(&line).to_string()),
    };

    let warning = lines
        .next_line()
        .await
        .ok()
        .flatten()
        .and_then(|line| line.strip_prefix("warn ").map(str::to_string));

    Ok((path, warning))
}
