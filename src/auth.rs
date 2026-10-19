//! Bounded administrator sessions, CSRF validation and password verification.

use crate::error::{Error, Result};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::{HeaderMap, StatusCode};
use rand_core::{OsRng, RngCore};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Absolute session lifetime; activity does not extend a stolen cookie's validity.
const LIFETIME: Duration = Duration::from_secs(8 * 3600);

/// A server-side session carried only by an HttpOnly cookie.
#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub csrf: String,
    pub expires: Instant,
}

/// Small in-memory stores; restarting the service revokes all sessions.
#[derive(Default)]
pub struct Auth {
    sessions: Mutex<HashMap<String, Session>>,
    attempts: Mutex<VecDeque<Instant>>,
}

impl Auth {
    /// Allows at most five password checks per minute across the single account.
    pub fn allow_attempt(&self) -> Result<()> {
        let now = Instant::now();
        let mut attempts = self.attempts.lock().map_err(Error::internal)?;
        attempts.retain(|at| now.duration_since(*at) < Duration::from_secs(60));
        if attempts.len() >= 5 {
            return Err(Error(
                StatusCode::TOO_MANY_REQUESTS,
                "Liian monta kirjautumisyritystä. Odota minuutti.".into(),
            ));
        }
        attempts.push_back(now);
        Ok(())
    }

    /// Creates a session, evicting the oldest when the sixteen-session bound is reached.
    pub fn create(&self) -> Result<Session> {
        let session = Session {
            id: token(),
            csrf: token(),
            expires: Instant::now() + LIFETIME,
        };
        let mut sessions = self.sessions.lock().map_err(Error::internal)?;
        sessions.retain(|_, s| s.expires > Instant::now());
        if sessions.len() >= 16 {
            if let Some(id) = sessions
                .values()
                .min_by_key(|s| s.expires)
                .map(|s| s.id.clone())
            {
                sessions.remove(&id);
            }
        }
        sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    /// Finds an unexpired session; also used to revoke active console connections.
    pub fn find(&self, id: &str) -> Option<Session> {
        let mut sessions = self.sessions.lock().ok()?;
        sessions.retain(|_, s| s.expires > Instant::now());
        sessions.get(id).cloned()
    }

    /// Extracts only this application's cookie and checks its server-side record.
    pub fn session_from_headers(&self, headers: &HeaderMap) -> Option<Session> {
        let cookies = headers.get("cookie")?.to_str().ok()?;
        cookies
            .split(';')
            .filter_map(|part| part.trim().split_once('='))
            .find(|(name, _)| *name == "vvw_session")
            .and_then(|(_, id)| self.find(id))
    }

    /// Revokes a session immediately, including its open consoles.
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.sessions.lock().map_err(Error::internal)?.remove(id);
        Ok(())
    }

    /// Revokes every session so graceful shutdown also terminates active console bridges.
    pub fn revoke_all(&self) -> Result<()> {
        self.sessions.lock().map_err(Error::internal)?.clear();
        Ok(())
    }
}

impl Session {
    /// Rejects cross-site or stale mutation forms before touching libvirt.
    pub fn check_csrf(&self, supplied: &str) -> Result<()> {
        if supplied != self.csrf {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Lomake on vanhentunut. Lataa sivu uudelleen.".into(),
            ));
        }
        Ok(())
    }
}

/// Uses OS randomness to produce a 256-bit session or form token.
pub fn token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Verifies an Argon2id hash without implementing custom cryptography.
pub fn verify(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Generates an administrator hash for a password supplied through standard input.
pub fn hash_password(password: &str) -> Result<String> {
    if password.len() < 12 || password.len() > 1024 {
        return Err(Error::bad(
            "Password must contain between 12 and 1024 bytes",
        ));
    }
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(Error::internal)
}

/// Builds a host-only cookie with a fixed path and no JavaScript access.
pub fn cookie(id: &str, secure: bool, clear: bool) -> String {
    format!(
        "vvw_session={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        if clear { 0 } else { LIFETIME.as_secs() },
        if secure { "; Secure" } else { "" }
    )
}

/// Session lifetime and service shutdown regression tests.
#[cfg(test)]
mod tests {
    use super::*;

    /// Expired cookies are refused and service shutdown revokes all remaining sessions.
    #[test]
    fn expiration_and_service_shutdown_revoke_access() {
        let auth = Auth::default();
        let expired = auth.create().unwrap();
        auth.sessions.lock().unwrap().get_mut(&expired.id).unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert!(auth.find(&expired.id).is_none());
        let active = auth.create().unwrap();
        auth.revoke_all().unwrap();
        assert!(auth.find(&active.id).is_none());
    }
}
