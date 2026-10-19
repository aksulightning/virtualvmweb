//! Authenticated VNC WebSocket bridge using libvirt's Unix socket handoff.

use crate::{
    App,
    auth::Session,
    backend,
    error::{Error, Result},
    xml,
};
use axum::{
    Extension,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use std::{
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Opens only the authenticated guest's VNC device; clients cannot submit a host or port.
pub async fn upgrade(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    if headers.get("origin").and_then(|v| v.to_str().ok())
        != Some(app.config.public_origin.as_str())
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Konsolin alkuperä ei täsmää.".into(),
        ));
    }
    let permit = app
        .consoles
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::conflict("Enintään neljä konsolia voi olla auki kerrallaan."))?;
    let guest = id.clone();
    let stream = app
        .backend
        .run(move |conn| {
            let dom = backend::domain(conn, &guest)?;
            if !dom.is_active()? {
                return Err(Error::conflict("Kone ei ole käynnissä."));
            }
            let index = xml::vnc_index(&xml::parse(&dom.get_xml_desc(0)?)?)
                .ok_or_else(|| Error::bad("Koneella ei ole VNC-näyttöä."))?;
            let (client, server) = UnixStream::pair().map_err(Error::internal)?;
            // Authorization is enforced by this app and libvirt; no public VNC port is needed.
            dom.open_graphics(
                index,
                server.as_raw_fd(),
                virt::sys::VIR_DOMAIN_OPEN_GRAPHICS_SKIPAUTH,
            )?;
            client.set_nonblocking(true).map_err(Error::internal)?;
            Ok(client)
        })
        .await?;
    let stream = tokio::net::UnixStream::from_std(stream).map_err(Error::internal)?;
    Ok(ws
        .max_message_size(1024 * 1024)
        .max_frame_size(1024 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            tracing::info!(guest = %id, "administrator opened console");
            if let Err(error) = bridge(socket, stream, app, &session.id).await {
                tracing::debug!(%error, guest = %id, "console disconnected");
            }
        }))
}

/// Relays binary VNC frames with backpressure and revokes access within fifteen seconds of logout.
async fn bridge(
    mut socket: WebSocket,
    mut stream: tokio::net::UnixStream,
    app: App,
    session: &str,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 16384];
    let mut check = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            _ = check.tick() => {
                if app.auth.find(session).is_none() { break; }
            }
            bytes = stream.read(&mut buffer) => {
                let count = bytes?;
                if count == 0 { break; }
                match tokio::time::timeout(Duration::from_secs(10), socket.send(Message::Binary(buffer[..count].to_vec().into()))).await {
                    Ok(Ok(())) => (), _ => break,
                }
            }
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        if !matches!(tokio::time::timeout(Duration::from_secs(10), stream.write_all(&bytes)).await, Ok(Ok(()))) { break; }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => (),
                    _ => break,
                }
            }
        }
    }
    Ok(())
}
