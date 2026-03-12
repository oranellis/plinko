//! Mutable state for the overview page.

use crate::data::ids::NodeId;
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
    // Momentum / animation state
    pub vel_x: f32,
    pub vel_y: f32,
    pub zoom_vel: f32,
    /// Target zoom for smooth interpolation (shift+scroll nudges this).
    pub zoom_target: f32,
    // Drag tracking
    pub is_dragging: bool,
    pub last_drag_x: f32,
    pub last_drag_y: f32,
    pub drag_vel_x: f32,
    pub drag_vel_y: f32,
    // Horizontal scroll (pixels)
    pub scroll_x: f32,
    // Settings window flag
    pub open_settings_window: bool,
    // Stored plan data for initialising the settings window
    pub settings_init_name: String,
    pub settings_init_date: String,
    pub settings_init_scheduler_target: NodeId,
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
            vel_x: 0.0,
            vel_y: 0.0,
            zoom_vel: 1.0,
            zoom_target: GANTT_ZOOM_DEFAULT,
            is_dragging: false,
            last_drag_x: 0.0,
            last_drag_y: 0.0,
            drag_vel_x: 0.0,
            drag_vel_y: 0.0,
            scroll_x: 0.0,
            open_settings_window: false,
            settings_init_name: String::new(),
            settings_init_date: String::new(),
            settings_init_scheduler_target: NodeId::PlanStart,
        }
    }
}
