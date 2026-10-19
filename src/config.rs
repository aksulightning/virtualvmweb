//! Strict TOML configuration and deployment boundary validation.

use std::{net::SocketAddr, path::PathBuf};
use argon2::PasswordHash;
use serde::Deserialize;
use crate::error::{Error, Result};

/// Administrator-controlled settings; credentials never come from browser input.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub public_origin: String,
    pub libvirt_uri: String,
    pub admin_user: String,
    pub password_hash: String,
    pub iso_dir: PathBuf,
    pub novnc_dir: PathBuf,
}

impl Config {
    /// Reads a configuration file and rejects unsafe or ambiguous settings.
    pub fn read(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(Error::internal)?;
        let config: Self = toml::from_str(&text).map_err(Error::internal)?;
        config.validate()?;
        Ok(config)
    }

    /// Restricts production to local libvirt and requires HTTPS for remote access.
    pub fn validate(&self) -> Result<()> {
        let uri: axum::http::Uri = self.public_origin.parse().map_err(Error::internal)?;
        let scheme = uri.scheme_str().unwrap_or("");
        let host = uri.host().unwrap_or("");
        if !matches!(scheme, "http" | "https") || host.is_empty()
            || uri.authority().is_some_and(|a| a.as_str().contains('@'))
            || self.public_origin.ends_with('/')
            || uri.path_and_query().is_some_and(|p| p.as_str() != "/") {
            return Err(Error::bad("public_origin must be a bare http(s) origin without a trailing slash"));
        }
        if scheme == "http" && (!self.listen.ip().is_loopback() || !matches!(host, "localhost" | "127.0.0.1" | "[::1]")) {
            return Err(Error::bad("Remote access requires an HTTPS public_origin"));
        }
        if !matches!(self.libvirt_uri.as_str(), "qemu:///system" | "test:///default") {
            return Err(Error::bad("Only qemu:///system and the test driver are supported"));
        }
        let hash = PasswordHash::new(&self.password_hash).map_err(Error::internal)?;
        if hash.algorithm.as_str() != "argon2id" || self.admin_user.is_empty() {
            return Err(Error::bad("Configure an administrator and an Argon2id password hash"));
        }
        if !self.iso_dir.is_absolute() || !self.novnc_dir.is_absolute() {
            return Err(Error::bad("Asset and ISO directories must be absolute paths"));
        }
        Ok(())
    }

    /// Returns the exact expected Host header, including any non-default port.
    pub fn authority(&self) -> &str {
        self.public_origin.split_once("://").map_or("", |(_, host)| host)
    }

    /// Secure cookies are required whenever the browser-facing origin uses TLS.
    pub fn secure(&self) -> bool {
        self.public_origin.starts_with("https://")
    }
}

