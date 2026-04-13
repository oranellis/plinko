//! Authentication database — users, sessions, admin management.
//!
//! Backed by a local SQLite file (`auth.db`) in the application data dir.
//! Passwords are stored as bcrypt hashes (cost 12).

use bcrypt::{DEFAULT_COST, hash, verify};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub user_id: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    UserNotFound,
    UsernameTaken,
    InvalidEmail,
    SessionExpired,
    SessionNotFound,
    WrongPassword,
    Db(rusqlite::Error),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCredentials => write!(f, "Invalid email or password"),
            Self::UserNotFound => write!(f, "User not found"),
            Self::UsernameTaken => write!(f, "Email address already registered"),
            Self::InvalidEmail => write!(f, "Username must be a valid email address"),
            Self::SessionExpired => write!(f, "Session expired — please log in again"),
            Self::SessionNotFound => write!(f, "Session not found — please log in again"),
            Self::WrongPassword => write!(f, "Incorrect current password"),
            Self::Db(e) => write!(f, "Database error: {e}"),
        }
    }
}

impl From<rusqlite::Error> for AuthError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

/// Thread-safe auth database handle.
#[derive(Clone)]
pub struct AuthDb {
    inner: Arc<Mutex<Connection>>,
}

impl AuthDb {
    /// Open (or create) the auth database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        create_schema(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Return the recommended path for the auth DB given the storage plans dir.
    pub fn default_path(plans_dir: &Path) -> PathBuf {
        // plans_dir is .../plinko/plans — auth.db goes one level up
        plans_dir.parent().unwrap_or(plans_dir).join("auth.db")
    }

    /// Ensure a root user exists. If no users are in the database, creates
    /// `root@localhost` with password `root` (admin) and prints a warning to stderr.
    pub fn bootstrap_root(&self) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        if count == 0 {
            let id = Uuid::new_v4().to_string();
            let password_hash = hash("root", DEFAULT_COST).expect("bcrypt hash failed");
            conn.execute(
                "INSERT INTO users (id, username, password_hash, is_admin) VALUES (?1, ?2, ?3, 1)",
                params![id, "root@localhost", password_hash],
            )?;
            eprintln!("=======================================================");
            eprintln!("  PLINKO: First-time setup — root user created.");
            eprintln!("  Email:    root@localhost");
            eprintln!("  Password: root");
            eprintln!("  Please change this password after logging in!");
            eprintln!("=======================================================");
        }
        Ok(())
    }

    /// Validate credentials. Returns `SessionInfo` on success.
    pub fn login(&self, email: &str, password: &str) -> Result<(String, SessionInfo), AuthError> {
        let conn = self.inner.lock().unwrap();
        let row: Option<(String, String, bool)> = conn
            .query_row(
                "SELECT id, password_hash, is_admin FROM users WHERE username = ?1",
                params![email],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        let (user_id, hash_stored, is_admin) = row.ok_or(AuthError::InvalidCredentials)?;

        if !verify(password, &hash_stored).unwrap_or(false) {
            return Err(AuthError::InvalidCredentials);
        }

        // Create session (7-day expiry).
        let token = Uuid::new_v4().to_string();
        let expires = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(7))
            .unwrap()
            .to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (token, user_id, expires_at) VALUES (?1, ?2, ?3)",
            params![token, user_id, expires],
        )?;

        Ok((
            token,
            SessionInfo {
                user_id,
                email: email.to_string(),
                is_admin,
            },
        ))
    }

    /// Validate a session token. Returns `SessionInfo` if valid and not expired.
    pub fn authenticate_token(&self, token: &str) -> Result<SessionInfo, AuthError> {
        let conn = self.inner.lock().unwrap();
        let row: Option<(String, String, bool, String)> = conn
            .query_row(
                "SELECT u.id, u.username, u.is_admin, s.expires_at
                 FROM sessions s JOIN users u ON s.user_id = u.id
                 WHERE s.token = ?1",
                params![token],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let (user_id, email, is_admin, expires_at) = row.ok_or(AuthError::SessionNotFound)?;

        // Check expiry.
        let exp = chrono::DateTime::parse_from_rfc3339(&expires_at)
            .map_err(|_| AuthError::SessionExpired)?;
        if chrono::Utc::now() > exp {
            return Err(AuthError::SessionExpired);
        }

        Ok(SessionInfo {
            user_id,
            email,
            is_admin,
        })
    }

    /// Invalidate a session token (logout).
    pub fn logout(&self, token: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    /// List all users (admin operation).
    pub fn list_users(&self) -> Result<Vec<AuthUser>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, username, is_admin FROM users ORDER BY username")?;
        let users = stmt
            .query_map([], |r| {
                Ok(AuthUser {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    is_admin: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    /// Create a new user. `email` is used as the login username. Returns the new user's UUID.
    pub fn create_user(
        &self,
        email: &str,
        password: &str,
        is_admin: bool,
    ) -> Result<String, AuthError> {
        validate_email(email)?;
        let id = Uuid::new_v4().to_string();
        let password_hash = hash(password, DEFAULT_COST).expect("bcrypt hash failed");
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, password_hash, is_admin) VALUES (?1, ?2, ?3, ?4)",
            params![id, email, password_hash, is_admin],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref sql_err, _) = e
                && sql_err.code == rusqlite::ErrorCode::ConstraintViolation
            {
                return AuthError::UsernameTaken;
            }
            AuthError::Db(e)
        })?;
        Ok(id)
    }

    /// Update a user's email and/or admin flag. Only admins can call this.
    pub fn update_user(
        &self,
        user_id: &str,
        new_email: Option<&str>,
        new_is_admin: Option<bool>,
    ) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        if let Some(email) = new_email {
            validate_email(email)?;
            conn.execute(
                "UPDATE users SET username = ?1 WHERE id = ?2",
                params![email, user_id],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(ref sql_err, _) = e
                    && sql_err.code == rusqlite::ErrorCode::ConstraintViolation
                {
                    return AuthError::UsernameTaken;
                }
                AuthError::Db(e)
            })?;
        }
        if let Some(is_admin) = new_is_admin {
            conn.execute(
                "UPDATE users SET is_admin = ?1 WHERE id = ?2",
                params![is_admin, user_id],
            )?;
        }
        Ok(())
    }

    /// Set a user's password (admin override — no old password needed).
    pub fn set_password(&self, user_id: &str, new_password: &str) -> Result<(), AuthError> {
        let hash_new = hash(new_password, DEFAULT_COST).expect("bcrypt hash failed");
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            params![hash_new, user_id],
        )?;
        // Invalidate all sessions for this user.
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id])?;
        Ok(())
    }

    /// Change own password — requires correct old password.
    pub fn change_own_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), AuthError> {
        let stored_hash: String = {
            let conn = self.inner.lock().unwrap();
            conn.query_row(
                "SELECT password_hash FROM users WHERE id = ?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(AuthError::UserNotFound)?
        };
        if !verify(old_password, &stored_hash).unwrap_or(false) {
            return Err(AuthError::WrongPassword);
        }
        let hash_new = hash(new_password, DEFAULT_COST).expect("bcrypt hash failed");
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            params![hash_new, user_id],
        )?;
        Ok(())
    }

    /// Delete a user and all their sessions.
    pub fn delete_user(&self, user_id: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id])?;
        conn.execute("DELETE FROM users WHERE id = ?1", params![user_id])?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Plan visibility
    // -------------------------------------------------------------------------

    /// Return the list of user IDs that have explicit access to `plan_id`.
    /// An empty vec means the plan is visible to all authenticated users.
    pub fn get_plan_visibility(&self, plan_id: Uuid) -> Result<Vec<String>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        let mut stmt = conn.prepare("SELECT user_id FROM plan_visibility WHERE plan_id = ?1")?;
        let ids = stmt
            .query_map(params![pid], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    /// Replace the visibility list for `plan_id`.
    /// Pass an empty slice to make the plan visible to all authenticated users.
    pub fn set_plan_visibility(&self, plan_id: Uuid, user_ids: &[String]) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        conn.execute(
            "DELETE FROM plan_visibility WHERE plan_id = ?1",
            params![pid],
        )?;
        for uid in user_ids {
            conn.execute(
                "INSERT OR IGNORE INTO plan_visibility (plan_id, user_id) VALUES (?1, ?2)",
                params![pid, uid],
            )?;
        }
        Ok(())
    }

    /// Filter `plan_ids` to only those visible to the given user.
    /// Admins always see all plans.
    /// For non-admins: plans with no visibility entries are visible to all;
    /// plans with entries are only visible to listed users.
    pub fn filter_visible_plans(
        &self,
        user_id: &str,
        is_admin: bool,
        plan_ids: &[Uuid],
    ) -> Vec<Uuid> {
        if is_admin {
            return plan_ids.to_vec();
        }
        let conn = self.inner.lock().unwrap();
        plan_ids
            .iter()
            .copied()
            .filter(|pid| {
                let pid_str = pid.to_string();
                // Count entries for this plan
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM plan_visibility WHERE plan_id = ?1",
                        params![pid_str],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if count == 0 {
                    return true; // no restriction — visible to all
                }
                // Check if this user is in the list
                let allowed: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM plan_visibility WHERE plan_id = ?1 AND user_id = ?2",
                        params![pid_str, user_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                allowed > 0
            })
            .collect()
    }
    // -------------------------------------------------------------------------
    // User preferences
    // -------------------------------------------------------------------------

    /// Get the last active plan ID for a user, if any.
    pub fn get_user_last_plan(&self, user_id: &str) -> Option<Uuid> {
        let conn = self.inner.lock().unwrap();
        conn.query_row(
            "SELECT last_plan_id FROM user_prefs WHERE user_id = ?1",
            params![user_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten()
        .and_then(|s| Uuid::parse_str(&s).ok())
    }

    /// Persist the last active plan ID for a user.
    pub fn set_user_last_plan(
        &self,
        user_id: &str,
        plan_id: Option<Uuid>,
    ) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let plan_id_str = plan_id.map(|u| u.to_string());
        conn.execute(
            "INSERT OR REPLACE INTO user_prefs (user_id, last_plan_id) VALUES (?1, ?2)",
            params![user_id, plan_id_str],
        )?;
        Ok(())
    }
}

fn create_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            id            TEXT PRIMARY KEY,
            username      TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            is_admin      INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token      TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
        CREATE TABLE IF NOT EXISTS plan_visibility (
            plan_id  TEXT NOT NULL,
            user_id  TEXT NOT NULL,
            PRIMARY KEY (plan_id, user_id)
        );
        CREATE TABLE IF NOT EXISTS user_prefs (
            user_id      TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            last_plan_id TEXT
        );",
    )
}

/// Validate that `s` is a plausible email address (local@domain.tld).
/// This is intentionally permissive — it just checks structure, not deliverability.
fn validate_email(s: &str) -> Result<(), AuthError> {
    let s = s.trim();
    // Must contain exactly one '@'
    let at = s.find('@').ok_or(AuthError::InvalidEmail)?;
    if s[..at].is_empty() {
        return Err(AuthError::InvalidEmail); // empty local part
    }
    let domain = &s[at + 1..];
    if domain.is_empty() {
        return Err(AuthError::InvalidEmail);
    }
    // Domain must contain at least one '.' with something on both sides
    let dot = domain.rfind('.').ok_or(AuthError::InvalidEmail)?;
    if dot == 0 || dot + 1 >= domain.len() {
        return Err(AuthError::InvalidEmail);
    }
    // No spaces anywhere
    if s.contains(' ') {
        return Err(AuthError::InvalidEmail);
    }
    Ok(())
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
