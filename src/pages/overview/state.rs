//! Mutable state for the overview page.

use crate::ui::layout::GANTT_ZOOM_DEFAULT;

/// Full interactive state for the overview page.
pub struct OverviewState {
    /// Hovered page-specific toolbar button index, if any.
    pub toolbar_btn_hovered: Option<usize>,
    /// Set when the users toolbar button is clicked; consumed by `take_open_request`.
    pub open_users_window: bool,
    /// Set when the task (plus) toolbar button is clicked; consumed by `take_open_request`.
    pub open_task_form: bool,
    /// Set when the milestone (diamond) toolbar button is clicked; consumed by `take_open_request`.
    pub open_milestone_form: bool,

    // ── Gantt chart state ──────────────────────────────────────────────────
    /// Vertical scroll offset in pixels.
    pub scroll_y: f32,
    /// Zoom level in pixels per day. Shift+scroll adjusts this.
    pub zoom: f32,
    /// Last known cursor position (used for hover).
    pub cursor_x: f32,
    pub cursor_y: f32,
}

impl OverviewState {
    pub fn new() -> Self {
        Self {
            toolbar_btn_hovered: None,
            open_users_window: false,
            open_task_form: false,
            open_milestone_form: false,
            scroll_y: 0.0,
            zoom: GANTT_ZOOM_DEFAULT,
            cursor_x: 0.0,
            cursor_y: 0.0,
        }
    }
}
