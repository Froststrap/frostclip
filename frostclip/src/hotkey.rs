use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::ipc::SaveRequest;

const SAVE_ID: &str = "save-clip";

pub fn spawn(tx: mpsc::Sender<SaveRequest>) {
    tokio::spawn(async move {
        if let Err(err) = run(tx).await {
            warn!("HOTKEY: global shortcuts unavailable: {}", err);
        }
    });
}

async fn run(tx: mpsc::Sender<SaveRequest>) -> ashpd::Result<()> {
    let shortcuts = GlobalShortcuts::new().await?;
    let session = shortcuts.create_session(Default::default()).await?;
    let mut activated = shortcuts.receive_activated().await?;

    let shortcut = NewShortcut::new(SAVE_ID, "Save replay clip").preferred_trigger("CTRL+ALT+F10");

    let request = shortcuts
        .bind_shortcuts(&session, &[shortcut], None, Default::default())
        .await?;

    let bound = request.response()?;

    for shortcut in bound.shortcuts() {
        info!(
            "HOTKEY: bound id={} trigger={}",
            shortcut.id(),
            shortcut.trigger_description()
        );
    }

    while let Some(event) = activated.next().await {
        if event.shortcut_id() != SAVE_ID {
            continue;
        }

        let (reply, receiver) = oneshot::channel();

        if tx
            .send(SaveRequest {
                seconds: None,
                reply,
            })
            .await
            .is_err()
        {
            break;
        }

        tokio::spawn(async move {
            match receiver.await {
                Ok(Ok((path, _))) => info!("HOTKEY: clip saved to {}", path),
                Ok(Err(message)) => warn!("HOTKEY: save failed: {}", message),
                Err(_) => warn!("HOTKEY: engine stopped"),
            }
        });
    }

    Ok(())
}
