use ashpd::desktop::{
    PersistMode,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};
use std::os::fd::OwnedFd;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub struct PortalStream {
    pub screencast: Screencast,
    pub session: ashpd::desktop::Session<Screencast>,
    pub streams: Vec<ashpd::desktop::screencast::Stream>,
    pub pipewire_fd: OwnedFd,
    pub restore_token: Option<String>,
}

pub async fn create(saved_restore_token: Option<&str>) -> ashpd::Result<PortalStream> {
    info!("PORTAL: creating screencast proxy");
    let screencast = Screencast::new().await?;

    info!("PORTAL: creating session");
    let session = screencast.create_session(Default::default()).await?;

    let mut options = SelectSourcesOptions::default()
        .set_cursor_mode(CursorMode::Embedded)
        .set_sources(SourceType::Monitor | SourceType::Window);

    if let Some(token) = saved_restore_token {
        if !token.is_empty() {
            info!("PORTAL: restoring session with token");
            options = options
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token);
        }
    }

    info!("PORTAL: selecting sources");
    let request = screencast.select_sources(&session, options).await?;
    request.response()?;

    info!("PORTAL: starting capture session");
    let request = screencast.start(&session, None, Default::default()).await?;
    let response = request.response()?;

    let streams = response.streams().to_vec();
    if streams.is_empty() {
        warn!("PORTAL: portal returned zero streams");
    } else {
        for (i, stream) in streams.iter().enumerate() {
            debug!(
                "PORTAL: stream[{}] node_id={}",
                i,
                stream.pipe_wire_node_id()
            );
        }
    }

    let new_restore_token = response.restore_token().map(|t| t.to_string());
    if new_restore_token.is_some() {
        info!("PORTAL: received restore token");
    }

    info!("PORTAL: opening pipewire remote");
    let pipewire_fd = screencast
        .open_pipe_wire_remote(&session, Default::default())
        .await?;

    info!("PORTAL: session established successfully");

    Ok(PortalStream {
        screencast,
        session,
        streams,
        pipewire_fd,
        restore_token: new_restore_token,
    })
}
