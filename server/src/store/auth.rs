//! Password hashes and sessions, kept apart from the tenant so no secret reaches an update, a
//! snapshot or a client.

// modules {{{
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    common::error::{PlinkoError, PlinkoResult},
    store::Store,
};
// }}}

// constants {{{
pub const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
// }}}

// helpers {{{
fn now() -> i64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
}

/// Only a hash of a session token is stored, so a copy of the file cannot be used to log in
fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
// }}}

// impl Store {{{
impl Store {
    pub fn set_password(&self, user: Uuid, password: &str) -> PlinkoResult<()> {
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
            .map_err(|e| PlinkoError::Store(e.to_string()))?;
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| PlinkoError::Store(e.to_string()))?
            .to_string();
        self.db().execute(
            "INSERT OR REPLACE INTO credentials (user_id, hash) VALUES (?1, ?2)",
            params![user.as_bytes().to_vec(), hash],
        )?;
        Ok(())
    }

    /// A user with no password never matches
    pub fn check_password(&self, user: Uuid, password: &str) -> PlinkoResult<bool> {
        let hash: Option<String> = self
            .db()
            .query_row(
                "SELECT hash FROM credentials WHERE user_id = ?1",
                params![user.as_bytes().to_vec()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(hash) = hash else {
            // hash anyway, so the timing does not reveal who has an account
            let salt = SaltString::encode_b64(Uuid::nil().as_bytes())
                .map_err(|e| PlinkoError::Store(e.to_string()))?;
            let _ = Argon2::default().hash_password(password.as_bytes(), &salt);
            return Ok(false);
        };
        let parsed = PasswordHash::new(&hash).map_err(|e| PlinkoError::Store(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }

    /// The only time the token is known, as just its hash is stored
    pub fn start_session(&self, user: Uuid) -> PlinkoResult<String> {
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let expires = now() + i64::try_from(SESSION_LIFETIME.as_secs()).unwrap_or(i64::MAX);
        self.db().execute(
            "INSERT INTO sessions (token_hash, user_id, expires) VALUES (?1, ?2, ?3)",
            params![token_hash(&token), user.as_bytes().to_vec(), expires],
        )?;
        Ok(token)
    }

    pub fn session_user(&self, token: &str) -> PlinkoResult<Option<Uuid>> {
        let user: Option<Vec<u8>> = self
            .db()
            .query_row(
                "SELECT user_id FROM sessions WHERE token_hash = ?1 AND expires > ?2",
                params![token_hash(token), now()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(user.and_then(|bytes| Uuid::from_slice(&bytes).ok()))
    }

    pub fn end_session(&self, token: &str) -> PlinkoResult<()> {
        let db = self.db();
        db.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![token_hash(token)],
        )?;
        db.execute("DELETE FROM sessions WHERE expires <= ?1", params![now()])?;
        Ok(())
    }

    pub fn end_sessions_of(&self, user: Uuid) -> PlinkoResult<()> {
        self.db().execute(
            "DELETE FROM sessions WHERE user_id = ?1",
            params![user.as_bytes().to_vec()],
        )?;
        Ok(())
    }
}
// }}}
