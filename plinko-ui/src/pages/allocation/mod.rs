//! Allocation page — per-user daily workload view.

pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
use crate::ui::layout::{
    ALLOC_USER_ENTRY_H, ALLOC_USER_PANEL_W, GANTT_ZOOM_MAX, GANTT_ZOOM_MIN, TOOLBAR_BTN_SIZE,
    TOOLBAR_BTN_Y,
};
use crate::ui::users_window::UsersWindow;
use plinko_shared::data::Plan;

use state::AllocationState;

pub struct AllocationPage {
    pub state: AllocationState,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl AllocationPage {
    pub fn new() -> Self {
        Self {
            state: AllocationState::new(),
        }
    }

    fn content_top() -> f32 {
        TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0
    }

    fn max_user_panel_scroll(num_users: usize, height: f32) -> f32 {
        let content_h = num_users as f32 * ALLOC_USER_ENTRY_H;
        let visible_h = height - Self::content_top();
        (content_h - visible_h).max(0.0)
    }
}
// }}}

// ── Page trait ─────────────────────────────────────────────────────────────── {{{
impl Page for AllocationPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        render::draw_allocation(canvas, width, height, &self.state, cache, plan);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion {
        let prev_x = self.state.cursor_x;
        let prev_y = self.state.cursor_y;
        self.state.cursor_x = x;
        self.state.cursor_y = y;

        let new_hover = render::hit_test_toolbar_buttons(x, y, width);
        let toolbar_dirty = new_hover != self.state.toolbar_btn_hovered;
        self.state.toolbar_btn_hovered = new_hover;

        // User panel hover
        let sorted_users = render::sorted_users_with_util_pub(plan);
        let new_user_hover =
            render::hit_test_user_panel(x, y, height, &self.state, &sorted_users).copied();
        let user_hover_dirty = new_user_hover != self.state.hovered_user;
        self.state.hovered_user = new_user_hover;

        // Task row hover (only when a user is selected and cursor is in timeline)
        let new_task_hover = if let Some(uid) = &self.state.selected_user {
            render::hit_test_task_row(x, y, plan, uid)
        } else {
            None
        };
        let task_hover_dirty = new_task_hover != self.state.hovered_task_idx;
        self.state.hovered_task_idx = new_task_hover;

        // Drag scrolling (timeline x only)
        if self.state.is_dragging {
            let dx = prev_x - x;
            let _dy = prev_y - y;
            self.state.scroll_x += dx;
            let alpha = 0.4_f32;
            self.state.drag_vel_x = self.state.drag_vel_x * (1.0 - alpha) + dx * alpha;
            return DirtyRegion::PageOnly;
        }

        if toolbar_dirty || user_hover_dirty || task_hover_dirty {
            DirtyRegion::PageOnly
        } else {
            DirtyRegion::None
        }
    }

    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        plan: &Plan,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        let is_toolbar = y <= TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE;
        let in_user_panel = x <= ALLOC_USER_PANEL_W;

        if pressed {
            if is_toolbar {
                match render::hit_test_toolbar_buttons(x, y, width) {
                    Some(0) => {
                        use chrono::Local;
                        let today = Local::now().date_naive();
                        let days = (today - plan.start_date).num_days();
                        self.state.scroll_x = days as f32 * self.state.zoom - (width * 0.5);
                        self.state.vel_x = 0.0;
                    }
                    Some(1) => self.state.open_users_window = true,
                    Some(2) => {
                        self.state.settings_init_name = plan.name.clone();
                        self.state.settings_init_date = plan.start_date.to_string();
                        self.state.settings_init_scheduler_target = plan.scheduler_target;
                        self.state.open_settings_window = true;
                    }
                    _ => {}
                }
            } else if in_user_panel {
                // User selection click
                let sorted_users = render::sorted_users_with_util_pub(plan);
                if let Some(uid) =
                    render::hit_test_user_panel(x, y, height, &self.state, &sorted_users)
                {
                    if self.state.selected_user.as_ref() == Some(uid) {
                        // Deselect on second click
                        self.state.selected_user = None;
                    } else {
                        self.state.selected_user = Some(*uid);
                        self.state.hovered_task_idx = None;
                    }
                }
            } else {
                // Timeline drag
                self.state.is_dragging = true;
                self.state.press_start_x = x;
                self.state.press_start_y = y;
                self.state.drag_vel_x = 0.0;
                self.state.vel_x = 0.0;
            }
        } else if self.state.is_dragging {
            self.state.is_dragging = false;
            if self.state.drag_vel_x.abs() > 1.5 {
                self.state.vel_x = self.state.drag_vel_x * 3.0;
            }
        }
        DirtyRegion::PageOnly
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        shift: bool,
        _width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion {
        if self.state.cursor_x <= ALLOC_USER_PANEL_W {
            // Scroll user panel vertically
            let max_scroll = Self::max_user_panel_scroll(plan.users_data.len(), height);
            self.state.user_panel_scroll =
                (self.state.user_panel_scroll - delta_y * 3.0).clamp(0.0, max_scroll);
        } else if shift {
            let factor = if delta_y > 0.0 {
                1.025_f32
            } else {
                1.0 / 1.025
            };
            self.state.zoom_target =
                (self.state.zoom_target * factor).clamp(GANTT_ZOOM_MIN, GANTT_ZOOM_MAX);
        } else {
            self.state.vel_x -= delta_y * 4.0;
            self.state.vel_x = self.state.vel_x.clamp(-300.0, 300.0);
        }
        DirtyRegion::PageOnly
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if self.state.open_users_window {
            self.state.open_users_window = false;
            return Some(Box::new(UsersWindow::new()));
        }
        if self.state.open_settings_window {
            self.state.open_settings_window = false;
            let w = crate::ui::plan_settings_window::PlanSettingsWindow::with_values(
                &self.state.settings_init_name,
                &self.state.settings_init_date,
                self.state.settings_init_scheduler_target,
            );
            return Some(Box::new(w));
        }
        None
    }

    fn reset_hover(&mut self) {
        self.state.toolbar_btn_hovered = None;
        self.state.hovered_user = None;
        self.state.hovered_task_idx = None;
    }

    fn has_animation(&self) -> bool {
        self.state.vel_x.abs() > 0.1 || (self.state.zoom_target - self.state.zoom).abs() > 0.05
    }

    fn tick_animation(&mut self, width: f32, _height: f32, _plan: &Plan) -> DirtyRegion {
        let friction = 0.88_f32;
        let mut dirty = false;

        let zoom_diff = self.state.zoom_target - self.state.zoom;
        if zoom_diff.abs() > 0.01 {
            let old_zoom = self.state.zoom;
            self.state.zoom += zoom_diff * 0.18;
            let ratio = self.state.zoom / old_zoom;
            let pivot_x = if self.state.cursor_x > ALLOC_USER_PANEL_W {
                self.state.cursor_x
            } else {
                width * 0.5
            };
            self.state.scroll_x = (pivot_x + self.state.scroll_x) * ratio - pivot_x;
            dirty = true;
        } else if zoom_diff.abs() > f32::EPSILON {
            self.state.zoom = self.state.zoom_target;
        }

        if self.state.vel_x.abs() > 0.1 {
            self.state.scroll_x += self.state.vel_x;
            self.state.vel_x *= friction;
            dirty = true;
        }

        if dirty {
            DirtyRegion::PageOnly
        } else {
            DirtyRegion::None
        }
    }
}
// }}}
