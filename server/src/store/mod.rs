//! A tenant's SQLite file: the log of updates applied to it, snapshots of the tenant so loading
//! does not replay everything, and the logins (see `auth`). Rows are written once and the log is
//! never rewritten, which makes the file an incremental backup of every change.

pub mod auth;

use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    codec,
    multitenancy::{tenant::Tenant, tenant_update::TenantUpdate},
    types::error::{PlinkoError, PlinkoResult},
};

/// A snapshot is taken every this many updates
const SNAPSHOT_EVERY: u64 = 200;

/// Snapshots older than the newest few are dropped
const SNAPSHOTS_KEPT: u64 = 3;

impl From<rusqlite::Error> for PlinkoError {
    fn from(error: rusqlite::Error) -> Self {
        PlinkoError::Store(error.to_string())
    }
}

pub struct Store {
    pub(crate) db: Mutex<Connection>,
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
             CREATE TABLE IF NOT EXISTS credentials (user_id BLOB PRIMARY KEY, hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS sessions (
                 token_hash BLOB PRIMARY KEY, user_id BLOB NOT NULL, expires INTEGER NOT NULL);",
        )?;
        Ok(Self { db: Mutex::new(db) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().expect("a store call does not panic")
    }

    /// Whether anything has been saved
    pub fn is_empty(&self) -> PlinkoResult<bool> {
        let any = self
            .db()
            .query_row("SELECT 1 FROM snapshots LIMIT 1", [], |_| Ok(()));
        Ok(any.optional()?.is_none())
    }

    /// Saves the first state of a tenant, as update 0
    pub fn create(&self, tenant: &Tenant) -> PlinkoResult<()> {
        self.snapshot(0, tenant)
    }

    /// The tenant as of the last update, and that update's number. It has no schedule, see
    /// `scheduler::schedule`.
    pub fn load(&self) -> PlinkoResult<(Tenant, u64)> {
        let newest = self.db().query_row(
            "SELECT seq, tenant FROM snapshots ORDER BY seq DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        );
        let (mut seq, bytes) = newest
            .optional()?
            .ok_or_else(|| PlinkoError::Store("nothing has been saved".to_string()))?;
        let mut tenant: Tenant = codec::decode(&bytes)?;
        let updates = {
            let db = self.db();
            let mut query =
                db.prepare("SELECT seq, update_bytes FROM log WHERE seq > ?1 ORDER BY seq")?;
            let rows = query.query_map(params![seq], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (next, bytes) in updates {
            tenant.apply_update(codec::decode(&bytes)?)?;
            seq = next;
        }
        Ok((tenant, seq))
    }

    /// Applies the update to the tenant and, only if it is allowed, saves it as the next update,
    /// returning its number. A snapshot follows every so many updates.
    pub fn commit(&self, tenant: &mut Tenant, seq: u64, update: TenantUpdate) -> PlinkoResult<u64> {
        let bytes = codec::encode(&update)?;
        tenant.apply_update(update)?;
        let seq = seq + 1;
        self.db().execute(
            "INSERT INTO log (seq, update_bytes) VALUES (?1, ?2)",
            params![seq, bytes],
        )?;
        if seq.is_multiple_of(SNAPSHOT_EVERY) {
            self.snapshot(seq, tenant)?;
        }
        Ok(seq)
    }

    /// The updates after `seq`, for a client that has the tenant as of `seq`
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
        let bytes = codec::encode(tenant)?;
        let db = self.db();
        db.execute(
            "INSERT OR REPLACE INTO snapshots (seq, tenant) VALUES (?1, ?2)",
            params![seq, bytes],
        )?;
        db.execute(
            "DELETE FROM snapshots WHERE seq <= (SELECT MAX(seq) FROM snapshots) - ?1 * ?2",
            params![SNAPSHOTS_KEPT, SNAPSHOT_EVERY],
        )?;
        Ok(())
    }
}
// }}}
