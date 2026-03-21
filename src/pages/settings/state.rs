//! Mutable state for the settings page.

use uuid::Uuid;

use crate::data::ids::UserId;

/// A saved plan shown in the plan list.
pub struct PlanEntry {
    pub id: Uuid,
    pub name: String,
    pub last_saved: String,
    pub is_current: bool,
}

/// All mutable state for the settings page.
pub struct SettingsState {
    /// Loaded plan summaries (populated by Application on navigate-to).
    pub plan_list: Vec<PlanEntry>,
    /// Which plan row (if any) is currently hovered.
    pub hovered_plan_row: Option<usize>,
    /// Which plan row's load button (if any) is currently hovered.
    pub hovered_load_btn: Option<usize>,
    /// Whether the "Save" button is hovered.
    pub hovered_save: bool,
    /// Whether the "New Plan" button is hovered.
    pub hovered_new: bool,
    /// Which user row (if any) is currently hovered in the identity section.
    pub hovered_user_idx: Option<usize>,
    /// Scroll offset for the content area.
    pub scroll_y: f32,
    /// Currently selected user ID (set from Application).
    pub current_user: Option<UserId>,

    // ── Pending actions consumed by Application ────────────────────────────
    /// App should save the current plan.
    pub pending_save: bool,
    /// App should create a new plan (replacing the current one).
    pub pending_new: bool,
    /// App should load this plan by UUID.
    pub pending_load: Option<Uuid>,
    /// App should change the current user (`Some(None)` → clear, `Some(Some(id))` → set).
    pub pending_set_user: Option<Option<UserId>>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Default for SettingsState {
    fn default() -> Self {
        Self {
            plan_list: Vec::new(),
            hovered_plan_row: None,
            hovered_load_btn: None,
            hovered_save: false,
            hovered_new: false,
            hovered_user_idx: None,
            scroll_y: 0.0,
            current_user: None,
            pending_save: false,
            pending_new: false,
            pending_load: None,
            pending_set_user: None,
        }
    }
}
// }}}
