use crate::data::plan::Plan;
use crate::monday::MondayConfig;
use crate::protocol::UserLink;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn binary_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string())
}

#[derive(Debug)]
pub enum StorageError {
    NoHomeDir,
    NoVersions,
    Io(std::io::Error),
    Json(serde_json::Error),
    MsgPack(Box<dyn std::error::Error + Send + Sync>),
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeDir => write!(f, "cannot determine home directory"),
            Self::NoVersions => write!(f, "no saved versions found for this plan"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::MsgPack(e) => write!(f, "MessagePack error: {e}"),
        }
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
// }}}
// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
// }}}

#[derive(Serialize, Deserialize, Default)]
struct AppConfig {
    monday_api_token: Option<String>,
}

#[derive(Clone)]
pub struct Storage {
    base: PathBuf,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Storage {
    pub fn from_path(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

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
        Ok(Self {
            base: base.join(binary_name()).join("plans"),
        })
    }

    pub fn plans_dir(&self) -> &PathBuf {
        &self.base
    }

    pub fn plan_dir(&self, plan_id: Uuid) -> PathBuf {
        self.base.join(plan_id.to_string())
    }

    pub fn save(&self, plan: &Plan) -> Result<PathBuf, StorageError> {
        let dir = self.plan_dir(plan.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.msgpack", Self::version_stamp()));
        let bytes =
            rmp_serde::to_vec_named(plan).map_err(|e| StorageError::MsgPack(Box::new(e)))?;
        fs::write(&path, bytes)?;
        Ok(path)
    }

    pub fn load_latest(&self, plan_id: Uuid) -> Result<Plan, StorageError> {
        let versions = self.list_versions(plan_id)?;
        let latest = versions.last().ok_or(StorageError::NoVersions)?;
        self.load_version(plan_id, latest)
    }

    pub fn load_version(&self, plan_id: Uuid, version: &str) -> Result<Plan, StorageError> {
        let path = self.plan_dir(plan_id).join(format!("{version}.msgpack"));
        let bytes = fs::read(path)?;
        rmp_serde::from_slice(&bytes).map_err(|e| StorageError::MsgPack(Box::new(e)))
    }

    pub fn list_plans(&self) -> Result<Vec<Uuid>, StorageError> {
        if !self.base.exists() {
            return Ok(vec![]);
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy())
            {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub fn delete_plan(&self, plan_id: Uuid) -> Result<(), StorageError> {
        let dir = self.plan_dir(plan_id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn list_versions(&self, plan_id: Uuid) -> Result<Vec<String>, StorageError> {
        let dir = self.plan_dir(plan_id);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut versions = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".msgpack") && entry.file_type()?.is_file() {
                let stem = name.trim_end_matches(".msgpack");
                // Only include timestamped snapshots (YYYY-MM-DDTHH-MM-SS).
                // This excludes monday.json and any other metadata files that
                // share the plan directory.
                if Self::is_version_stamp(stem) {
                    versions.push(stem.to_string());
                }
            }
        }
        versions.sort();
        Ok(versions)
    }

    fn is_version_stamp(s: &str) -> bool {
        // Matches exactly YYYY-MM-DDTHH-MM-SS (19 chars, digits and separators).
        s.len() == 19
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.as_bytes()[10] == b'T'
            && s.as_bytes()[13] == b'-'
            && s.as_bytes()[16] == b'-'
            && s[..4].chars().all(|c| c.is_ascii_digit())
            && s[5..7].chars().all(|c| c.is_ascii_digit())
            && s[8..10].chars().all(|c| c.is_ascii_digit())
            && s[11..13].chars().all(|c| c.is_ascii_digit())
            && s[14..16].chars().all(|c| c.is_ascii_digit())
            && s[17..19].chars().all(|c| c.is_ascii_digit())
    }

    fn version_stamp() -> String {
        Local::now().format("%Y-%m-%dT%H-%M-%S").to_string()
    }

    fn config_path(&self) -> PathBuf {
        self.base
            .parent()
            .map(|p| p.join("config.json"))
            .unwrap_or_else(|| self.base.join("config.json"))
    }

    fn load_config(&self) -> AppConfig {
        let path = self.config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        }
    }

    fn save_config(&self, config: &AppConfig) {
        let path = self.config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(config) {
            let _ = fs::write(path, json);
        }
    }

    pub fn plan_summary(&self, plan_id: Uuid) -> Option<(String, String)> {
        let versions = self.list_versions(plan_id).ok()?;
        let latest = versions.last()?.clone();
        let plan = self.load_version(plan_id, &latest).ok()?;
        Some((plan.name, latest))
    }

    // ── Monday.com config ────────────────────────────────────────────────────

    pub fn load_monday_config(&self, plan_id: Uuid) -> Option<MondayConfig> {
        let path = self.plan_dir(plan_id).join("monday.json");
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save_monday_config(&self, plan_id: Uuid, config: &MondayConfig) {
        let dir = self.plan_dir(plan_id);
        let _ = fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(config) {
            let _ = fs::write(dir.join("monday.json"), json);
        }
    }

    pub fn load_monday_api_token(&self) -> String {
        self.load_config().monday_api_token.unwrap_or_default()
    }

    pub fn save_monday_api_token(&self, token: &str) {
        let mut config = self.load_config();
        config.monday_api_token = if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        };
        self.save_config(&config);
    }

    pub fn load_user_links(&self, plan_id: Uuid) -> Vec<UserLink> {
        let path = self.plan_dir(plan_id).join("user_links.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_user_links(&self, plan_id: Uuid, links: &[UserLink]) {
        let dir = self.plan_dir(plan_id);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("user_links.json");
        if let Ok(data) = serde_json::to_string_pretty(links) {
            let _ = std::fs::write(path, data);
        }
    }
}
// }}}

#[cfg(test)]
mod tests {
    use super::Storage;

    #[test]
    fn is_version_stamp_accepts_valid() {
        assert!(Storage::is_version_stamp("2026-04-10T09-30-05"));
        assert!(Storage::is_version_stamp("2000-01-01T00-00-00"));
    }

    #[test]
    fn is_version_stamp_rejects_metadata_files() {
        assert!(!Storage::is_version_stamp("monday"));
        assert!(!Storage::is_version_stamp("config"));
        assert!(!Storage::is_version_stamp("2026-04-10"));
        assert!(!Storage::is_version_stamp(""));
    }
}
