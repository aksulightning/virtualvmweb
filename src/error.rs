//! HTTP errors with escaped public messages and detailed server-side diagnostics.

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

/// An expected request failure or a safely reported infrastructure error.
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);

/// Application operations return an HTTP-aware error.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Reports invalid input without performing a mutation.
    pub fn bad(message: &str) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }

    /// Reports a state conflict, such as editing a running guest.
    pub fn conflict(message: &str) -> Self {
        Self(StatusCode::CONFLICT, message.into())
    }

    /// Logs internal details while returning a generic message to the browser.
    pub fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "internal operation failed");
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Palvelutoiminto epäonnistui. Tarkista virtualvmweb-palvelun loki.".into(),
        )
    }
}

impl From<virt::error::Error> for Error {
    /// Keeps libvirt diagnostics in the service log.
    fn from(value: virt::error::Error) -> Self {
        Self::internal(value)
    }
}

impl IntoResponse for Error {
    /// Escapes messages before inserting them into an HTML error page.
    fn into_response(self) -> Response {
        let body = format!(
            "<section class=panel><h1>Toiminto ei onnistunut</h1><p>{}</p><a href=/>Palaa yleiskuvaan</a></section>",
            crate::views::escape(&self.1)
        );
        (self.0, Html(crate::views::document("Virhe", &body))).into_response()
    }
}
