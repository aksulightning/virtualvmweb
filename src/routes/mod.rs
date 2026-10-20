//! HTTP routing, authentication middleware and browser security headers.

/// Mutating forms and creation wizard endpoints.
mod mutations;
/// Authenticated inventory and guest detail pages.
mod pages;

use crate::{
    App,
    auth::{self, Session},
    console,
    error::{Error, Result},
    views,
};
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tower_http::services::ServeDir;

/// Builds public assets and authenticated routes under the same origin.
pub fn router(app: App) -> Router {
    let protected = Router::new()
        .route("/", get(pages::dashboard))
        .route("/machines", get(pages::machines).post(mutations::create))
        .route("/machines/new", get(mutations::new))
        .route("/machines/preview", post(mutations::preview))
        .route("/machines/{id}", get(pages::detail))
        .route("/machines/{id}/action", post(mutations::action))
        .route(
            "/machines/{id}/edit",
            get(mutations::edit_page).post(mutations::edit),
        )
        .route("/machines/{id}/console", get(pages::console_page))
        .route("/machines/{id}/ws", get(console::upgrade))
        .route("/api/machines/{id}", get(pages::machine_json))
        .route("/fragments/machines", get(pages::machines_fragment))
        .route("/fragments/dashboard", get(pages::dashboard_fragment))
        .route("/storage", get(pages::storage))
        .route("/volumes/delete", post(mutations::delete_volume))
        .route("/networks", get(pages::networks))
        .route("/settings", get(pages::settings))
        .route("/logout", post(logout))
        .nest_service("/novnc", ServeDir::new(&app.config.novnc_dir))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_session));
    Router::new()
        .merge(protected)
        .route("/login", get(login_page).post(login))
        .route("/healthz", get(health))
        .route("/assets/style.css", get(style))
        .route("/assets/app.js", get(script))
        .route("/assets/console.js", get(console_script))
        .route("/assets/audio.js", get(audio_script))
        .route("/assets/audio-rfb.js", get(audio_protocol))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), security))
        .with_state(app)
}

/// Requires the configured host and same-origin mutations, then applies browser protections.
async fn security(State(app): State<App>, request: Request, next: Next) -> Response {
    let host_ok =
        request.headers().get("host").and_then(|v| v.to_str().ok()) == Some(app.config.authority());
    let mutation = !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    let origin_ok = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        == Some(app.config.public_origin.as_str());
    let mut response = if !host_ok || (mutation && !origin_ok) {
        Error(
            StatusCode::FORBIDDEN,
            "Pyynnön alkuperä ei täsmää palvelun asetuksiin.".into(),
        )
        .into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    // no-referrer makes native form POSTs send Origin: null, failing our origin check.
    headers.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    if app.config.secure() {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}

/// Attaches a validated session before any protected handler or static console asset runs.
async fn require_session(State(app): State<App>, mut request: Request, next: Next) -> Response {
    if let Some(session) = app.auth.session_from_headers(request.headers()) {
        request.extensions_mut().insert(session);
        next.run(request).await
    } else if request.uri().path().starts_with("/api/") || request.uri().path().ends_with("/ws") {
        StatusCode::UNAUTHORIZED.into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

/// Renders the public login page without revealing account existence.
async fn login_page() -> Html<String> {
    Html(views::login(""))
}

/// Browser login input; body and field sizes are bounded before password hashing.
#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

/// Verifies credentials off the runtime threads, rotates sessions and sets a host-only cookie.
async fn login(
    State(app): State<App>,
    headers: axum::http::HeaderMap,
    Form(input): Form<Login>,
) -> Result<Response> {
    app.auth.allow_attempt()?;
    if input.username.len() > 128 || input.password.len() > 1024 {
        return Err(Error::bad("Kirjautumistiedot ovat liian pitkät."));
    }
    let permit = app
        .passwords
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::conflict("Kirjautumispalvelu on varattu."))?;
    let hash = app.config.password_hash.clone();
    let username_ok = input.username == app.config.admin_user;
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        auth::verify(&hash, &input.password) && username_ok
    })
    .await
    .map_err(Error::internal)?;
    if !valid {
        tracing::warn!("administrator login failed");
        return Ok((
            StatusCode::UNAUTHORIZED,
            Html(views::login("Käyttäjätunnus tai salasana on väärä.")),
        )
            .into_response());
    }
    if let Some(old) = app.auth.session_from_headers(&headers) {
        app.auth.revoke(&old.id)?;
    }
    let session = app.auth.create()?;
    tracing::info!("administrator logged in");
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        "set-cookie",
        auth::cookie(&session.id, app.config.secure(), false)
            .parse()
            .map_err(Error::internal)?,
    );
    Ok(response)
}

/// CSRF field for forms without other parameters.
#[derive(Deserialize)]
struct Csrf {
    csrf: String,
}

/// Revokes the server-side session before clearing the browser cookie.
async fn logout(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Form(input): Form<Csrf>,
) -> Result<Response> {
    session.check_csrf(&input.csrf)?;
    app.auth.revoke(&session.id)?;
    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        "set-cookie",
        auth::cookie("", app.config.secure(), true)
            .parse()
            .map_err(Error::internal)?,
    );
    Ok(response)
}

/// Liveness only: daemon readiness is shown by authenticated inventory requests.
async fn health() -> &'static str {
    "ok\n"
}

/// Serves the embedded stylesheet without requiring a frontend build tool.
async fn style() -> impl IntoResponse {
    (
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../../assets/style.css"),
    )
}

/// Serves progressive enhancement for filters, form feedback and polling.
async fn script() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../../assets/app.js"),
    )
}

/// Serves the application-owned noVNC integration module.
async fn console_script() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../../assets/console.js"),
    )
}

/// Serves playback controls without a frontend build dependency.
async fn audio_script() -> impl IntoResponse {
    ([("content-type", "text/javascript; charset=utf-8")], include_str!("../../assets/audio.js"))
}

/// Serves the bounded QEMU audio protocol decoder.
async fn audio_protocol() -> impl IntoResponse {
    ([("content-type", "text/javascript; charset=utf-8")], include_str!("../../assets/audio-rfb.js"))
}
