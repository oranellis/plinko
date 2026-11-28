//! A tenant's SQLite file: the update log, a daily snapshot so loading need not replay
//! everything, the schedules, and logins (see `auth`). Rows are append-only, so the tenant and
//! its schedule as of any update can be read back.

// modules {{{
pub mod auth;

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    codec,
    common::error::{PlinkoError, PlinkoResult},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{TenantUpdate, TenantUpdateType},
    },
    scheduler::solution::{Solution, Stored},
};
// }}}

// impl PlinkoError {{{
impl From<rusqlite::Error> for PlinkoError {
    fn from(error: rusqlite::Error) -> Self {
        PlinkoError::Store(error.to_string())
    }
}
// }}}

// tenant_dir {{{
/// Relative to the directory the server runs from
pub fn tenant_dir(name: &str) -> PlinkoResult<PathBuf> {
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if name.is_empty() || name.len() > 64 || !name.chars().all(plain) {
        let why = "a tenant name is up to 64 letters, digits, - and _";
        return Err(PlinkoError::Generic(format!(
            "{name:?} is not a valid name: {why}"
        )));
    }
    Ok(Path::new("data").join(name))
}
// }}}

pub struct Store {
    pub(crate) db: Mutex<Connection>,
    /// The next solution is stored as a diff against this one
    last: Mutex<Option<(u64, Solution)>>,
}

// impl Store {{{
impl Store {
    pub fn open(path: &Path) -> PlinkoResult<Self> {
        Self::with(Connection::open(path)?)
    }

    pub fn open_in_memory() -> PlinkoResult<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(db: Connection) -> PlinkoResult<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS log (seq INTEGER PRIMARY KEY, update_bytes BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS snapshots (seq INTEGER PRIMARY KEY, tenant BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS schedules (
                 seq INTEGER PRIMARY KEY, whole INTEGER NOT NULL, solution BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS credentials (user_id BLOB PRIMARY KEY, hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS sessions (
                 token_hash BLOB PRIMARY KEY, user_id BLOB NOT NULL, expires INTEGER NOT NULL);",
        )?;
        let store = Self {
            db: Mutex::new(db),
            last: Mutex::new(None),
        };
        let last = store.schedule_at(u64::MAX)?;
        *store.last.lock().expect("a store call does not panic") = last;
        Ok(store)
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().expect("a store call does not panic")
    }

    pub fn is_empty(&self) -> PlinkoResult<bool> {
        let any = self
            .db()
            .query_row("SELECT 1 FROM snapshots LIMIT 1", [], |_| Ok(()));
        Ok(any.optional()?.is_none())
    }

    /// Saved as update 0
    pub fn create(&self, tenant: &Tenant) -> PlinkoResult<()> {
        self.snapshot(0, tenant)
    }

    /// With no schedule, see `schedule_at` and `scheduler::restore`
    pub fn load(&self) -> PlinkoResult<(Tenant, u64)> {
        self.load_at(u64::MAX)
    }

    /// Returns the last update number at or before `at`
    pub fn load_at(&self, at: u64) -> PlinkoResult<(Tenant, u64)> {
        let at = at.min(i64::MAX as u64);
        let newest = self.db().query_row(
            "SELECT seq, tenant FROM snapshots WHERE seq <= ?1 ORDER BY seq DESC LIMIT 1",
            params![at],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        );
        let (mut seq, bytes) = newest
            .optional()?
            .ok_or_else(|| PlinkoError::Store("nothing has been saved".to_string()))?;
        let mut tenant: Tenant = codec::decode(&bytes)?;
        let updates = {
            let db = self.db();
            let mut query = db.prepare(
                "SELECT seq, update_bytes FROM log WHERE seq > ?1 AND seq <= ?2 ORDER BY seq",
            )?;
            let rows = query.query_map(params![seq, at], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (next, bytes) in updates {
            tenant.apply_logged(codec::decode(&bytes)?)?;
            seq = next;
        }
        Ok((tenant, seq))
    }

    /// Returns the update the solution was made for, earlier than `at` if nothing was scheduled
    /// since
    pub fn schedule_at(&self, at: u64) -> PlinkoResult<Option<(u64, Solution)>> {
        let at = at.min(i64::MAX as u64);
        let records = {
            let db = self.db();
            let mut query = db.prepare(
                "SELECT seq, solution FROM schedules WHERE seq <= ?1 AND seq >=
                     (SELECT MAX(seq) FROM schedules WHERE whole = 1 AND seq <= ?1)
                 ORDER BY seq",
            )?;
            let rows = query.query_map(params![at], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut found: Option<(u64, Solution)> = None;
        for (seq, bytes) in records {
            let stored: Stored = codec::decode(&bytes)?;
            let before = found.map(|(_, solution)| solution).unwrap_or_default();
            found = Some((seq, before.then(stored)));
        }
        Ok(found)
    }

    /// Stored as a diff, or whole if most of it changed or a snapshot was taken since, so each
    /// day's snapshot has a whole schedule to go with it
    pub fn save_schedule(&self, seq: u64, solution: &Solution) -> PlinkoResult<()> {
        let mut last = self.last.lock().expect("a store call does not panic");
        let snapshot: Option<u64> =
            self.db()
                .query_row("SELECT MAX(seq) FROM snapshots", [], |row| row.get(0))?;
        let previous = last
            .as_ref()
            .filter(|(at, _)| snapshot.is_none_or(|taken| taken <= *at));
        let stored = solution.stored_after(previous.map(|(_, solution)| solution));
        let whole = matches!(stored, Stored::Full(_));
        self.db().execute(
            "INSERT OR REPLACE INTO schedules (seq, whole, solution) VALUES (?1, ?2, ?3)",
            params![seq, whole, codec::encode(&stored)?],
        )?;
        *last = Some((seq, solution.clone()));
        Ok(())
    }

    /// On any failure the tenant is left as it was. A new day also takes a snapshot.
    pub fn commit(&self, tenant: &mut Tenant, seq: u64, update: TenantUpdate) -> PlinkoResult<u64> {
        let bytes = codec::encode(&update)?;
        let new_day = matches!(update.update_type, TenantUpdateType::SetToday(_));
        let mut next = tenant.clone();
        next.apply_update(update)?;
        let seq = seq + 1;
        let mut db = self.db();
        let saving = db.transaction()?;
        saving.execute(
            "INSERT INTO log (seq, update_bytes) VALUES (?1, ?2)",
            params![seq, bytes],
        )?;
        if new_day {
            insert_snapshot(&saving, seq, &next)?;
        }
        saving.commit()?;
        *tenant = next;
        Ok(seq)
    }

    pub fn since(&self, seq: u64) -> PlinkoResult<Vec<(u64, TenantUpdate)>> {
        let db = self.db();
        let mut query =
            db.prepare("SELECT seq, update_bytes FROM log WHERE seq > ?1 ORDER BY seq")?;
        let rows = query.query_map(params![seq], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut updates = Vec::new();
        for row in rows {
            let (seq, bytes) = row?;
            updates.push((seq, codec::decode(&bytes)?));
        }
        Ok(updates)
    }

    pub fn snapshot(&self, seq: u64, tenant: &Tenant) -> PlinkoResult<()> {
        insert_snapshot(&self.db(), seq, tenant)
    }
}
// }}}

// insert_snapshot {{{
fn insert_snapshot(db: &Connection, seq: u64, tenant: &Tenant) -> PlinkoResult<()> {
    db.execute(
        "INSERT OR REPLACE INTO snapshots (seq, tenant) VALUES (?1, ?2)",
        params![seq, codec::encode(tenant)?],
    )?;
    Ok(())
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenants_get_a_folder_under_data_and_names_cannot_leave_it() {
        assert_eq!(tenant_dir("Acme-1_x").unwrap(), Path::new("data/Acme-1_x"));
        for name in ["", "..", "a/b", "../x", "a b", ".hidden", &"n".repeat(65)] {
            assert!(tenant_dir(name).is_err(), "{name:?}");
        }
    }
}
// }}}
