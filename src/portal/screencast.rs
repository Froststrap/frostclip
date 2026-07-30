use ashpd::desktop::{
    PersistMode,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};
use std::os::fd::OwnedFd;
use tracing::info;

#[derive(Debug)]
pub struct PortalStream {
    pub screencast: Screencast,
    pub session: ashpd::desktop::Session<Screencast>,
    pub streams: Vec<ashpd::desktop::screencast::Stream>,
    pub pipewire_fd: OwnedFd,
    pub restore_token: Option<String>,
}

pub async fn create(saved_restore_token: Option<&str>) -> ashpd::Result<PortalStream> {
    let screencast = Screencast::new().await?;
    let session = screencast.create_session(Default::default()).await?;

    let mut options = SelectSourcesOptions::default()
        .set_cursor_mode(CursorMode::Embedded)
        .set_sources(SourceType::Monitor | SourceType::Window)
        .set_persist_mode(PersistMode::ExplicitlyRevoked);

    if let Some(token) = saved_restore_token {
        if !token.is_empty() {
            options = options.set_restore_token(token);
        }
    }

    let request = screencast.select_sources(&session, options).await?;
    request.response()?;

    let request = screencast.start(&session, None, Default::default()).await?;
    let response = request.response()?;

    let new_restore_token = response.restore_token().map(|t| t.to_string());
    if new_restore_token.is_some() {
        info!("PORTAL: received restore token");
    }

    let pipewire_fd = screencast
        .open_pipe_wire_remote(&session, Default::default())
        .await?;

    Ok(PortalStream {
        screencast,
        session,
        streams: response.streams().to_vec(),
        pipewire_fd,
        restore_token: new_restore_token,
    })
}
