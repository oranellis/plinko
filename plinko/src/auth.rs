//! Authentication database — users, sessions, admin management.
//!
//! Backed by a local SQLite file (`auth.db`) in the application data dir.
//! Passwords are stored as bcrypt hashes (cost 12).

use bcrypt::{DEFAULT_COST, hash, verify};
use plinko_shared::protocol::{OrgMembership, OrgRole};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub org_memberships: Vec<OrgMembership>,
    pub active_org_id: Option<String>,
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
    OtherError(String),
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
            Self::OtherError(msg) => write!(f, "{msg}"),
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
        let (token, user_id, is_admin, active_org_id_raw) = {
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

            // For non-admins, require at least one org membership.
            if !is_admin {
                let org_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM org_members WHERE user_id = ?1",
                        params![user_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if org_count == 0 {
                    return Err(AuthError::OtherError(
                        "Your account is not assigned to an organisation.                          Please contact an administrator."
                            .to_string(),
                    ));
                }
            }

            // Pick first org alphabetically as the default active org.
            let first_org: Option<String> = conn
                .query_row(
                    "SELECT o.id FROM org_members m                      JOIN organisations o ON m.org_id = o.id                      WHERE m.user_id = ?1 ORDER BY o.name LIMIT 1",
                    params![user_id],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);

            // Create session (7-day expiry).
            let token = Uuid::new_v4().to_string();
            let expires = chrono::Utc::now()
                .checked_add_signed(chrono::Duration::days(7))
                .unwrap()
                .to_rfc3339();
            conn.execute(
                "INSERT INTO sessions (token, user_id, expires_at, active_org_id)                  VALUES (?1, ?2, ?3, ?4)",
                params![token, user_id, expires, first_org],
            )?;
            (token, user_id, is_admin, first_org)
        }; // conn dropped here — lock released before re-acquiring in get_user_org_memberships

        let org_memberships = self.get_user_org_memberships(&user_id);
        let active_org_id = active_org_id_raw;
        Ok((
            token,
            SessionInfo {
                user_id,
                email: email.to_string(),
                is_admin,
                org_memberships,
                active_org_id,
            },
        ))
    }

    /// Validate a session token. Returns `SessionInfo` if valid and not expired.
    pub fn authenticate_token(&self, token: &str) -> Result<SessionInfo, AuthError> {
        let (user_id, email, is_admin, active_org_id_raw) = {
            let conn = self.inner.lock().unwrap();
            let row: Option<(String, String, bool, String, Option<String>)> = conn
                .query_row(
                    "SELECT u.id, u.username, u.is_admin, s.expires_at, s.active_org_id
                     FROM sessions s JOIN users u ON s.user_id = u.id
                     WHERE s.token = ?1",
                    params![token],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;

            let (user_id, email, is_admin, expires_at, active_org_id_raw) =
                row.ok_or(AuthError::SessionNotFound)?;

            // Check expiry.
            let exp = chrono::DateTime::parse_from_rfc3339(&expires_at)
                .map_err(|_| AuthError::SessionExpired)?;
            if chrono::Utc::now() > exp {
                return Err(AuthError::SessionExpired);
            }
            (user_id, email, is_admin, active_org_id_raw)
        }; // conn dropped here — lock released before re-acquiring in get_user_org_memberships

        let org_memberships = self.get_user_org_memberships(&user_id);

        // Validate and backfill active_org_id for all users (including admins).
        // If the stored org is missing or no longer valid, default to the first membership.
        let first_org = org_memberships.first().map(|m| m.org_id.clone());
        let valid = active_org_id_raw
            .as_ref()
            .map(|oid| org_memberships.iter().any(|m| &m.org_id == oid))
            .unwrap_or(false);
        let active_org_id = if valid {
            active_org_id_raw
        } else {
            // Backfill: update the session row to the first org.
            if let Some(ref fid) = first_org {
                let conn = self.inner.lock().unwrap();
                let _ = conn.execute(
                    "UPDATE sessions SET active_org_id = ?1 WHERE token = ?2",
                    params![fid, token],
                );
            }
            first_org
        };

        Ok(SessionInfo {
            user_id,
            email,
            is_admin,
            org_memberships,
            active_org_id,
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

    /// List users that are members of a specific organisation (includes is_admin flag).
    pub fn list_org_users(&self, org_id: &str) -> Result<Vec<AuthUser>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, u.is_admin FROM users u \
             JOIN org_members m ON m.user_id = u.id \
             WHERE m.org_id = ?1 ORDER BY u.username",
        )?;
        let users = stmt
            .query_map(params![org_id], |r| {
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
    /// Invalidates all other sessions for the user (keeps the current session active).
    pub fn change_own_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
        current_session_token: &str,
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
        conn.execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND token != ?2",
            params![user_id, current_session_token],
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

    /// Filter `plan_ids` to only those visible to the given user.
    /// Admins always see all plans.
    /// Non-admins can only see plans in orgs they are members of,
    /// unless the plan has an explicit NoAccess permission override.
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
                // Check if plan belongs to an org
                let org_id: Option<String> = conn
                    .query_row(
                        "SELECT org_id FROM plan_org WHERE plan_id = ?1",
                        params![pid_str],
                        |r| r.get(0),
                    )
                    .optional()
                    .unwrap_or(None);

                if let Some(org_id) = org_id {
                    // Plan is in an org — only visible to org members
                    let role: Option<String> = conn
                        .query_row(
                            "SELECT role FROM org_members WHERE org_id = ?1 AND user_id = ?2",
                            params![org_id, user_id],
                            |r| r.get(0),
                        )
                        .optional()
                        .unwrap_or(None);

                    return match role.as_deref() {
                        None => false, // not a member
                        Some("Admin") => true, // org admins bypass per-plan permissions
                        Some(_) => {
                            // Non-admin members: check for an explicit NoAccess override
                            let perm: Option<String> = conn
                                .query_row(
                                    "SELECT permission FROM plan_permissions WHERE plan_id = ?1 AND user_id = ?2",
                                    params![pid_str, user_id],
                                    |r| r.get(0),
                                )
                                .optional()
                                .unwrap_or(None);
                            perm.as_deref() != Some("NoAccess")
                        }
                    };
                }

                // No org — not accessible to non-admins (all plans must be in an org)
                false
            })
            .collect()
    }
    // -------------------------------------------------------------------------
    // Organisation management
    // -------------------------------------------------------------------------

    pub fn get_user_org_memberships(&self, user_id: &str) -> Vec<OrgMembership> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT o.id, o.name, m.role FROM org_members m \
                 JOIN organisations o ON m.org_id = o.id \
                 WHERE m.user_id = ?1 ORDER BY o.name",
            )
            .unwrap_or_else(|_| panic!("prepare failed"));
        stmt.query_map(params![user_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|(org_id, org_name, role_str)| {
            let role = match role_str.as_str() {
                "Admin" => OrgRole::Admin,
                "User" => OrgRole::User,
                _ => OrgRole::Viewer,
            };
            OrgMembership {
                org_id,
                org_name,
                role,
            }
        })
        .collect()
    }

    pub fn list_orgs(&self) -> Result<Vec<(String, String)>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name FROM organisations ORDER BY name")?;
        let orgs = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(orgs)
    }

    pub fn list_user_orgs(&self, user_id: &str) -> Result<Vec<(String, String)>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT o.id, o.name FROM organisations o \
             JOIN org_members m ON o.id = m.org_id \
             WHERE m.user_id = ?1 ORDER BY o.name",
        )?;
        let orgs = stmt
            .query_map(params![user_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(orgs)
    }

    pub fn create_org(&self, name: &str) -> Result<String, AuthError> {
        let id = Uuid::new_v4().to_string();
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO organisations (id, name) VALUES (?1, ?2)",
            params![id, name],
        )?;
        Ok(id)
    }

    pub fn delete_org(&self, org_id: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute("DELETE FROM organisations WHERE id = ?1", params![org_id])
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(ref sql_err, _) = e
                    && sql_err.code == rusqlite::ErrorCode::ConstraintViolation
                {
                    return AuthError::OtherError(
                        "Cannot delete an organisation that has plans assigned to it. \
                         Reassign or unassign plans first."
                            .to_string(),
                    );
                }
                AuthError::Db(e)
            })?;
        Ok(())
    }

    pub fn rename_org(&self, org_id: &str, name: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "UPDATE organisations SET name = ?1 WHERE id = ?2",
            params![name, org_id],
        )?;
        Ok(())
    }

    pub fn get_org_members(
        &self,
        org_id: &str,
    ) -> Result<Vec<(String, String, String)>, AuthError> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, m.role FROM org_members m \
             JOIN users u ON m.user_id = u.id \
             WHERE m.org_id = ?1 ORDER BY u.username",
        )?;
        let members = stmt
            .query_map(params![org_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(members)
    }

    pub fn set_org_member(&self, org_id: &str, user_id: &str, role: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO org_members (org_id, user_id, role) VALUES (?1, ?2, ?3)",
            params![org_id, user_id, role],
        )?;
        Ok(())
    }

    pub fn remove_org_member(&self, org_id: &str, user_id: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "DELETE FROM org_members WHERE org_id = ?1 AND user_id = ?2",
            params![org_id, user_id],
        )?;
        Ok(())
    }

    pub fn set_plan_org(&self, plan_id: Uuid, org_id: Option<&str>) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        if let Some(oid) = org_id {
            conn.execute(
                "INSERT OR REPLACE INTO plan_org (plan_id, org_id) VALUES (?1, ?2)",
                params![pid, oid],
            )?;
        } else {
            conn.execute("DELETE FROM plan_org WHERE plan_id = ?1", params![pid])?;
        }
        Ok(())
    }

    pub fn get_plan_org(&self, plan_id: Uuid) -> Option<String> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        conn.query_row(
            "SELECT org_id FROM plan_org WHERE plan_id = ?1",
            params![pid],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Returns all plan IDs that belong to the given organisation.
    pub fn get_plans_for_org(&self, org_id: &str) -> Vec<Uuid> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT plan_id FROM plan_org WHERE org_id = ?1") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![org_id], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect()
    }

    /// Returns true if the user is a member of the given organisation (any role).
    pub fn is_org_member(&self, user_id: &str, org_id: &str) -> bool {
        let conn = self.inner.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM org_members WHERE org_id = ?1 AND user_id = ?2",
                params![org_id, user_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count > 0
    }

    /// Returns the explicit plan-level permission for a user, if set.
    /// `None` means "inherit org role" (default).
    pub fn get_plan_permission(&self, plan_id: Uuid, user_id: &str) -> Option<String> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        conn.query_row(
            "SELECT permission FROM plan_permissions WHERE plan_id = ?1 AND user_id = ?2",
            params![pid, user_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Sets an explicit plan-level permission for a user.
    /// Pass `None` to delete the override (revert to inheriting the org role).
    pub fn set_plan_permission(
        &self,
        plan_id: Uuid,
        user_id: &str,
        permission: Option<&str>,
    ) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        match permission {
            Some(p) => {
                conn.execute(
                    "INSERT OR REPLACE INTO plan_permissions (plan_id, user_id, permission) VALUES (?1, ?2, ?3)",
                    params![pid, user_id, p],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM plan_permissions WHERE plan_id = ?1 AND user_id = ?2",
                    params![pid, user_id],
                )?;
            }
        }
        Ok(())
    }

    /// Returns explicit per-plan permissions for a user within the given org.
    /// Plans with no explicit row are omitted (they inherit the org role).
    pub fn get_user_permissions_for_org(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> HashMap<Uuid, String> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT pp.plan_id, pp.permission \
             FROM plan_permissions pp \
             JOIN plan_org po ON pp.plan_id = po.plan_id \
             WHERE po.org_id = ?1 AND pp.user_id = ?2",
        ) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        stmt.query_map(params![org_id, user_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .filter_map(|(s, p)| Uuid::parse_str(&s).ok().map(|id| (id, p)))
        .collect()
    }

    /// Removes all plan-level permission rows for a plan (call on plan deletion).
    pub fn delete_plan_permissions(&self, plan_id: Uuid) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let pid = plan_id.to_string();
        conn.execute(
            "DELETE FROM plan_permissions WHERE plan_id = ?1",
            params![pid],
        )?;
        Ok(())
    }

    pub fn get_user_org_role(&self, user_id: &str, org_id: &str) -> Option<OrgRole> {
        let conn = self.inner.lock().unwrap();
        let role_str: Option<String> = conn
            .query_row(
                "SELECT role FROM org_members WHERE org_id = ?1 AND user_id = ?2",
                params![org_id, user_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        role_str.map(|s| match s.as_str() {
            "Admin" => OrgRole::Admin,
            "User" => OrgRole::User,
            _ => OrgRole::Viewer,
        })
    }

    pub fn is_org_admin(&self, user_id: &str, org_id: &str) -> bool {
        matches!(
            self.get_user_org_role(user_id, org_id),
            Some(OrgRole::Admin)
        )
    }

    /// Returns true if the user is an admin of at least one organisation.
    pub fn is_any_org_admin(&self, user_id: &str) -> bool {
        let conn = self.inner.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM org_members WHERE user_id = ?1 AND role = 'Admin'",
                params![user_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count > 0
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

    // -------------------------------------------------------------------------
    // Active org management
    // -------------------------------------------------------------------------

    /// Switch a session's active organisation. The user must be a member.
    pub fn set_active_org(&self, token: &str, org_id: &str) -> Result<(), AuthError> {
        let conn = self.inner.lock().unwrap();
        let user_id: Option<String> = conn
            .query_row(
                "SELECT user_id FROM sessions WHERE token = ?1",
                params![token],
                |r| r.get(0),
            )
            .optional()?;
        let user_id = user_id.ok_or(AuthError::SessionNotFound)?;
        let is_member: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM org_members WHERE org_id = ?1 AND user_id = ?2",
                params![org_id, user_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if is_member == 0 {
            return Err(AuthError::OtherError(
                "Not a member of this organisation".to_string(),
            ));
        }
        conn.execute(
            "UPDATE sessions SET active_org_id = ?1 WHERE token = ?2",
            params![org_id, token],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Bug reports
    // -------------------------------------------------------------------------

    pub fn submit_bug_report(
        &self,
        user_id: &str,
        email: &str,
        description: &str,
        page_url: &str,
        user_agent: &str,
    ) -> Result<String, AuthError> {
        let id = Uuid::new_v4().to_string();
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO bug_reports              (id, user_id, email, description, page_url, user_agent)              VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, user_id, email, description, page_url, user_agent],
        )?;
        Ok(id)
    }

    pub fn list_bug_reports(&self) -> Vec<plinko_shared::protocol::BugReport> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, user_id, email, description, page_url, user_agent, submitted_at              FROM bug_reports ORDER BY submitted_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |r| {
            Ok(plinko_shared::protocol::BugReport {
                id: r.get(0)?,
                user_id: r.get(1)?,
                email: r.get(2)?,
                description: r.get(3)?,
                page_url: r.get(4)?,
                user_agent: r.get(5)?,
                submitted_at: r.get(6)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
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
        );
        CREATE TABLE IF NOT EXISTS organisations (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS org_members (
            org_id  TEXT NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role    TEXT NOT NULL CHECK(role IN ('Admin', 'User', 'Viewer')),
            PRIMARY KEY (org_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_org_members_user ON org_members(user_id);
        CREATE TABLE IF NOT EXISTS plan_org (
            plan_id TEXT PRIMARY KEY,
            org_id  TEXT NOT NULL REFERENCES organisations(id) ON DELETE RESTRICT
        );
        CREATE TABLE IF NOT EXISTS plan_permissions (
            plan_id    TEXT NOT NULL,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            permission TEXT NOT NULL CHECK(permission IN ('NoAccess', 'Viewer', 'User')),
            PRIMARY KEY (plan_id, user_id)
        );
        CREATE TABLE IF NOT EXISTS bug_reports (
            id           TEXT PRIMARY KEY,
            user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            email        TEXT NOT NULL,
            description  TEXT NOT NULL,
            page_url     TEXT NOT NULL,
            user_agent   TEXT NOT NULL,
            submitted_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    // Migration: add active_org_id column to sessions if it doesn't exist yet
    // (SQLite doesn't support IF NOT EXISTS for ALTER TABLE ADD COLUMN, so we catch
    // the duplicate-column error.)
    match conn.execute(
        "ALTER TABLE sessions ADD COLUMN active_org_id TEXT          REFERENCES organisations(id) ON DELETE SET NULL",
        [],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("duplicate column name") => {}
        Err(e) => return Err(e),
    }
    Ok(())
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
