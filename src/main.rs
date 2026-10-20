//! Entrypoint for the Alpine-only libvirt web service and password-hash utility.

/// Guest playback device configuration.
mod audio;
/// Administrator sessions and credential helpers.
mod auth;
/// Blocking libvirt operations.
mod backend;
/// Validated application settings.
mod config;
/// Authenticated browser-console bridge.
mod console;
/// Safe HTTP errors.
mod error;
/// Guest firmware selection and host support checks.
mod firmware;
/// Guest provisioning and offline edits.
mod provision;
/// HTTP routes and request security boundaries.
mod routes;
/// Pool and network inventory.
mod storage;
/// Server-rendered views.
mod views;
/// Structured libvirt XML handling.
mod xml;

use std::{io::Read, sync::Arc};
use tokio::sync::Semaphore;

/// Shared state with bounded libvirt, password-hashing and console concurrency.
#[derive(Clone)]
pub struct App {
    config: Arc<config::Config>,
    auth: Arc<auth::Auth>,
    backend: backend::Backend,
    consoles: Arc<Semaphore>,
    passwords: Arc<Semaphore>,
}

impl App {
    /// Creates independent session and libvirt state for one service instance.
    fn new(config: config::Config) -> Self {
        Self {
            backend: backend::Backend::new(config.libvirt_uri.clone()),
            config: Arc::new(config),
            auth: Arc::new(auth::Auth::default()),
            consoles: Arc::new(Semaphore::new(4)),
            passwords: Arc::new(Semaphore::new(2)),
        }
    }
}

/// Handles CLI utilities before starting the HTTP server.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "virtualvmweb=info".into()),
        )
        .init();
    if let Err(error) = run().await {
        tracing::error!(message = %error.1, "service failed");
        std::process::exit(1);
    }
    Ok(())
}

/// Starts the service only after the origin and administrator credentials are validated.
async fn run() -> error::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("virtualvmweb {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args == ["--hash-password"] {
        let mut password = String::new();
        std::io::stdin()
            .take(1026)
            .read_to_string(&mut password)
            .map_err(error::Error::internal)?;
        println!(
            "{}",
            auth::hash_password(password.trim_end_matches(['\r', '\n']))?
        );
        return Ok(());
    }
    let path = match args.as_slice() {
        [] => "/etc/virtualvmweb/config.toml",
        [flag, path] if flag == "--config" => path,
        _ => {
            return Err(error::Error::bad(
                "Usage: virtualvmweb [--config PATH | --hash-password | --version]",
            ));
        }
    };
    let config = config::Config::read(path)?;
    if !config.novnc_dir.join("core/rfb.js").is_file() {
        return Err(error::Error::bad(
            "noVNC assets missing; run scripts/fetch-novnc.sh and install the assets",
        ));
    }
    let listen = config.listen;
    let app = App::new(config);
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(error::Error::internal)?;
    tracing::info!(%listen, "virtualvmweb listening");
    axum::serve(listener, routes::router(app.clone()))
        .with_graceful_shutdown(shutdown(app.auth.clone()))
        .await
        .map_err(error::Error::internal)
}

/// Handles OpenRC's SIGTERM as well as Ctrl-C; process exit drops all session state.
async fn shutdown(auth: Arc<auth::Auth>) {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("register SIGTERM handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = terminate.recv() => () }
    if let Err(error) = auth.revoke_all() {
        tracing::error!(?error, "session revocation during shutdown failed");
    }
}

/// Regression tests for authorization, input boundaries and XML preservation.
#[cfg(test)]
mod tests;
