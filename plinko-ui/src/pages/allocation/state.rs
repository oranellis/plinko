//! Mutable state for the allocation page.

use crate::ui::layout::{GANTT_ZOOM_DEFAULT, GANTT_ZOOM_MAX, GANTT_ZOOM_MIN};
use plinko_shared::data::ids::{NodeId, UserId};

pub struct AllocationState {
    pub toolbar_btn_hovered: Option<usize>,
    pub scroll_x: f32,
    pub user_panel_scroll: f32,
    pub zoom: f32,
    pub zoom_target: f32,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub is_dragging: bool,
    pub drag_vel_x: f32,
    pub vel_x: f32,
    pub press_start_x: f32,
    pub press_start_y: f32,
    pub selected_user: Option<UserId>,
    pub hovered_user: Option<UserId>,
    pub hovered_task_idx: Option<usize>,
    pub open_settings_window: bool,
    pub open_users_window: bool,
    pub settings_init_name: String,
    pub settings_init_date: String,
    pub settings_init_scheduler_target: NodeId,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl AllocationState {
    pub fn new() -> Self {
        Self {
            toolbar_btn_hovered: None,
            scroll_x: 0.0,
            user_panel_scroll: 0.0,
            zoom: GANTT_ZOOM_DEFAULT,
            zoom_target: GANTT_ZOOM_DEFAULT,
            cursor_x: 0.0,
            cursor_y: 0.0,
            is_dragging: false,
            drag_vel_x: 0.0,
            vel_x: 0.0,
            press_start_x: 0.0,
            press_start_y: 0.0,
            selected_user: None,
            hovered_user: None,
            hovered_task_idx: None,
            open_settings_window: false,
            open_users_window: false,
            settings_init_name: String::new(),
            settings_init_date: String::new(),
            settings_init_scheduler_target: NodeId::PlanStart,
        }
    }

    pub fn max_zoom_target(&self) -> f32 {
        GANTT_ZOOM_MAX
    }

    pub fn min_zoom_target(&self) -> f32 {
        GANTT_ZOOM_MIN
    }
}
// }}}
