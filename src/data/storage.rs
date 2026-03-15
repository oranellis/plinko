use crate::data::ids::UserId;
use crate::data::plan::Plan;
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

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct AppConfig {
    current_user_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct Storage {
    base: PathBuf,
}

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
        let path = dir.join(format!("{}.json", Self::version_stamp()));
        fs::write(&path, serde_json::to_string_pretty(plan)?)?;
        Ok(path)
    }

    pub fn load_latest(&self, plan_id: Uuid) -> Result<Plan, StorageError> {
        let versions = self.list_versions(plan_id)?;
        let latest = versions.last().ok_or(StorageError::NoVersions)?;
        self.load_version(plan_id, latest)
    }

    pub fn load_version(&self, plan_id: Uuid, version: &str) -> Result<Plan, StorageError> {
        let path = self.plan_dir(plan_id).join(format!("{version}.json"));
        let json = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
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
        versions.sort();
        Ok(versions)
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

    pub fn load_current_user_id(&self) -> Option<UserId> {
        self.load_config().current_user_id.map(UserId)
    }

    pub fn save_current_user_id(&self, user_id: Option<UserId>) {
        let mut config = self.load_config();
        config.current_user_id = user_id.map(|u| u.0);
        self.save_config(&config);
    }

    pub fn plan_summary(&self, plan_id: Uuid) -> Option<(String, String)> {
        let versions = self.list_versions(plan_id).ok()?;
        let latest = versions.last()?.clone();
        let plan = self.load_version(plan_id, &latest).ok()?;
        Some((plan.name, latest))
    }
}
