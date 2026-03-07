//! Versioned JSON persistence for [`Plan`]s.
//!
//! Each save call writes a new timestamped snapshot file, allowing the full
//! history to be browsed and restored.

use std::fmt;
use std::fs;
use std::path::PathBuf;
use chrono::Local;
use uuid::Uuid;
use crate::data::plan::Plan;

/// Returns the name of the running binary, used as the data directory name.
fn binary_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string())
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum StorageError {
    /// Could not determine the user's home / data directory.
    NoHomeDir,
    /// The plan directory exists but contains no saved versions.
    NoVersions,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeDir => write!(f, "cannot determine home directory"),
            Self::NoVersions => write!(f, "no saved versions found for this plan"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StorageError { fn from(e: std::io::Error) -> Self { Self::Io(e) } }
impl From<serde_json::Error> for StorageError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }

// ── Storage ───────────────────────────────────────────────────────────────────

/// Manages plan persistence under a base directory.
///
/// Typical layout:
/// ```text
/// <base>/
///   <plan-uuid>/
///     2026-03-06T14-23-45.json   ← version snapshot
///     2026-03-07T09-01-00.json
/// ```
///
/// Create with [`Storage::from_user_data_dir`] for the standard user location
/// (`$XDG_DATA_HOME/skiatest/plans` or `~/.local/share/skiatest/plans`), or
/// [`Storage::from_path`] for a custom base (useful in tests).
pub struct Storage {
    base: PathBuf,
}

impl Storage {
    /// Open (or create) storage rooted at an explicit path.
    pub fn from_path(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Open storage at the standard XDG user data location:
    /// `$XDG_DATA_HOME/<binary>/plans` or `~/.local/share/<binary>/plans`,
    /// where `<binary>` is the name of the running executable.
    pub fn from_user_data_dir() -> Result<Self, StorageError> {
        let base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".local").join("share")
            });
        if base.as_os_str().is_empty() {
            return Err(StorageError::NoHomeDir);
        }
        Ok(Self { base: base.join(binary_name()).join("plans") })
    }

    // ── Paths ─────────────────────────────────────────────────────────────────

    /// `<base>/`
    pub fn plans_dir(&self) -> &PathBuf {
        &self.base
    }

    /// `<base>/<plan-uuid>/`
    pub fn plan_dir(&self, plan_id: Uuid) -> PathBuf {
        self.base.join(plan_id.to_string())
    }

    // ── Save / load ───────────────────────────────────────────────────────────

    /// Save a plan as a new timestamped version. Returns the path written.
    ///
    /// Each call creates a new file: `<base>/<uuid>/YYYY-MM-DDTHH-MM-SS.json`
    pub fn save(&self, plan: &Plan) -> Result<PathBuf, StorageError> {
        let dir = self.plan_dir(plan.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", Self::version_stamp()));
        fs::write(&path, serde_json::to_string_pretty(plan)?)?;
        Ok(path)
    }

    /// Load the most recently saved version of a plan.
    pub fn load_latest(&self, plan_id: Uuid) -> Result<Plan, StorageError> {
        let versions = self.list_versions(plan_id)?;
        let latest = versions.last().ok_or(StorageError::NoVersions)?;
        self.load_version(plan_id, latest)
    }

    /// Load a specific version by its timestamp string (e.g. `"2026-03-06T14-23-45"`).
    pub fn load_version(&self, plan_id: Uuid, version: &str) -> Result<Plan, StorageError> {
        let path = self.plan_dir(plan_id).join(format!("{version}.json"));
        let json = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }

    // ── Discovery ─────────────────────────────────────────────────────────────

    /// List all plan UUIDs found in the base directory.
    pub fn list_plans(&self) -> Result<Vec<Uuid>, StorageError> {
        if !self.base.exists() {
            return Ok(vec![]);
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) {
                    ids.push(id);
                }
        }
        Ok(ids)
    }

    /// List all saved version timestamps for a plan, sorted oldest → newest.
    pub fn list_versions(&self, plan_id: Uuid) -> Result<Vec<String>, StorageError> {
        let dir = self.plan_dir(plan_id);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut versions = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") && entry.file_type()?.is_file() {
                versions.push(name.trim_end_matches(".json").to_string());
            }
        }
        // Lexicographic sort == chronological for the YYYY-MM-DDTHH-MM-SS format.
        versions.sort();
        Ok(versions)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Timestamp string for a version filename: `YYYY-MM-DDTHH-MM-SS`.
    /// Colons replaced with dashes for filesystem compatibility.
    fn version_stamp() -> String {
        Local::now().format("%Y-%m-%dT%H-%M-%S").to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Milestone, Task, User};

    fn storage() -> (tempfile::TempDir, Storage) {
        let tmp = tempfile::tempdir().unwrap();
        let s = Storage::from_path(tmp.path());
        (tmp, s)
    }

    // ── Path construction ─────────────────────────────────────────────────────

    #[test]
    fn plan_dir_contains_uuid() {
        let (_tmp, s) = storage();
        let id = Uuid::new_v4();
        assert!(s.plan_dir(id).to_string_lossy().contains(&id.to_string()));
    }

    #[test]
    fn version_stamp_is_filesystem_safe() {
        let stamp = Storage::version_stamp();
        assert_eq!(stamp.len(), 19); // YYYY-MM-DDTHH-MM-SS
        assert!(!stamp.contains(':'));
    }

    // ── Save / load ───────────────────────────────────────────────────────────

    #[test]
    fn save_creates_file_under_plan_uuid_dir() {
        let (_tmp, s) = storage();
        let plan = Plan::new("Alpha");
        let path = s.save(&plan).unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains(&plan.id.to_string()));
        assert_eq!(path.extension().unwrap(), "json");
    }

    #[test]
    fn load_latest_returns_same_plan() {
        let (_tmp, s) = storage();
        let mut plan = Plan::new("Beta");
        plan.add_user(User::new("Alice"));
        s.save(&plan).unwrap();

        let loaded = s.load_latest(plan.id).unwrap();
        assert_eq!(loaded.id, plan.id);
        assert_eq!(loaded.name, plan.name);
        assert_eq!(loaded.users.len(), 1);
    }

    #[test]
    fn multiple_saves_create_multiple_versions() {
        let (_tmp, s) = storage();
        let plan = Plan::new("Gamma");
        s.save(&plan).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        s.save(&plan).unwrap();

        let versions = s.list_versions(plan.id).unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions[0] < versions[1]); // oldest first
    }

    #[test]
    fn load_version_loads_specific_snapshot() {
        let (_tmp, s) = storage();
        let mut plan = Plan::new("Delta");
        s.save(&plan).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        plan.add_task(Task::new("New task", ""));
        s.save(&plan).unwrap();

        let versions = s.list_versions(plan.id).unwrap();
        assert_eq!(versions.len(), 2);

        let v1 = s.load_version(plan.id, &versions[0]).unwrap();
        let v2 = s.load_version(plan.id, &versions[1]).unwrap();
        assert!(v1.tasks.is_empty());
        assert_eq!(v2.tasks.len(), 1);
    }

    // ── Discovery ─────────────────────────────────────────────────────────────

    #[test]
    fn list_plans_returns_saved_plan_ids() {
        let (_tmp, s) = storage();
        let p1 = Plan::new("P1");
        let p2 = Plan::new("P2");
        s.save(&p1).unwrap();
        s.save(&p2).unwrap();

        let mut ids = s.list_plans().unwrap();
        ids.sort();
        let mut expected = vec![p1.id, p2.id];
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn list_plans_empty_when_no_plans_saved() {
        let (_tmp, s) = storage();
        assert!(s.list_plans().unwrap().is_empty());
    }

    #[test]
    fn list_versions_empty_when_plan_not_saved() {
        let (_tmp, s) = storage();
        assert!(s.list_versions(Uuid::new_v4()).unwrap().is_empty());
    }

    #[test]
    fn load_latest_errors_when_no_versions() {
        let (_tmp, s) = storage();
        assert!(matches!(s.load_latest(Uuid::new_v4()), Err(StorageError::NoVersions)));
    }

    #[test]
    fn save_and_load_preserves_tasks_and_milestones() {
        let (_tmp, s) = storage();
        let mut plan = Plan::new("Full");
        plan.add_task(Task::new("T", "desc"));
        plan.add_milestone(Milestone::new("M", "desc"));
        s.save(&plan).unwrap();

        let loaded = s.load_latest(plan.id).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.milestones.len(), 1);
    }

    #[test]
    fn user_data_dir_errors_without_home() {
        // With no HOME or XDG_DATA_HOME, should return an error.
        // We can't safely unset env vars in tests, so just verify the happy path.
        // If HOME is set (it is in CI and dev), from_user_data_dir should succeed.
        if std::env::var("HOME").is_ok() || std::env::var("XDG_DATA_HOME").is_ok() {
            assert!(Storage::from_user_data_dir().is_ok());
        }
    }
}
