//! Monday.com integration configuration types.
//!
//! [`MondayConfig`] is stored per-plan as `plans/<plan-uuid>/monday.json`.
//! The API token is stored in the global `AppConfig` (`config.json`).

use serde::{Deserialize, Serialize};

use crate::data::allocation::Status;
use crate::data::ids::{NodeId, UserId};

/// Per-plan Monday.com configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MondayConfig {
    /// Monday board ID to sync with.
    pub board_id: String,
    /// Mapping from Monday column IDs to their semantic roles.
    pub column_map: ColumnMap,
    /// Mappings between Monday users and plinko users.
    pub user_mappings: Vec<UserMapping>,
    /// Mappings between Monday status labels and plinko Status values.
    pub status_mappings: Vec<StatusMapping>,
    /// Persistent mapping from Monday item IDs to plinko node IDs (for idempotent re-import).
    pub item_node_map: Vec<ItemNodeMapping>,
    /// When true, workload column values are in hours. When false, in days.
    pub workload_in_hours: bool,
}

/// Maps Monday column IDs to their semantic roles in plinko.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColumnMap {
    /// "people" type column — who is assigned.
    pub person_column_id: String,
    /// "status" type column — task status label.
    pub status_column_id: String,
    /// "dependency" type column — Monday inter-item dependencies.
    pub dependency_column_id: String,
    /// "numbers" type column — workload estimate (hours or days).
    pub workload_column_id: String,
    /// "timeline" type column — written on export with computed start/end dates.
    pub timeline_column_id: String,
    /// "date" type column — used to detect Monday milestone items (`is_milestone` flag).
    pub date_column_id: String,
}

/// Links a Monday.com workspace user to a plinko [`UserId`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMapping {
    pub monday_user_id: String,
    pub monday_name: String,
    /// `None` means this Monday user is unmapped (will be skipped on import).
    pub plinko_user_id: Option<UserId>,
}

/// Links a Monday.com status label to a plinko [`Status`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMapping {
    pub monday_label: String,
    pub plinko_status: Status,
}

/// Links a Monday.com item ID to a plinko node ID for idempotent re-import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemNodeMapping {
    pub monday_item_id: String,
    pub plinko_node_id: NodeId,
}

/// Describes a column on a Monday board (fetched from API, not persisted).
#[derive(Debug, Clone)]
pub struct BoardColumn {
    pub id: String,
    pub title: String,
    pub column_type: String,
}

/// A Monday.com workspace user (fetched from API, not persisted).
#[derive(Debug, Clone)]
pub struct MondayUser {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// A flat item imported from Monday (either a top-level item or a subitem).
#[derive(Debug, Clone)]
pub struct MondayItem {
    pub id: String,
    pub name: String,
    /// The parent item's ID, if this is a subitem.
    pub parent_id: Option<String>,
    /// Person column value — list of assigned user IDs.
    pub assigned_user_ids: Vec<String>,
    /// Status column text label.
    pub status_label: Option<String>,
    /// Dependency column — list of item IDs this item depends on.
    pub dependency_item_ids: Vec<String>,
    /// Workload estimate (hours or days).
    pub workload: Option<f32>,
    /// True when the Monday date column has `is_milestone: true` — import as a plinko milestone.
    pub is_milestone: bool,
}
