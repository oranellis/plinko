//! Allocation page — per-user daily workload view.

pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
use crate::ui::layout::{GANTT_ZOOM_MAX, GANTT_ZOOM_MIN, TOOLBAR_BTN_SIZE, TOOLBAR_BTN_Y};
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

    fn rows_top() -> f32 {
        use crate::ui::layout::GANTT_HEADER_H;
        TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0 + GANTT_HEADER_H
    }

    fn max_scroll_y(num_users: usize, height: f32) -> f32 {
        use crate::ui::layout::GANTT_ROW_H;
        let content_h = num_users as f32 * GANTT_ROW_H;
        let visible_h = height - Self::rows_top();
        (content_h - visible_h).max(0.0)
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Page for AllocationPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        render::draw_allocation(canvas, width, height, &self.state, cache, plan);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        _width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion {
        let prev_x = self.state.cursor_x;
        let prev_y = self.state.cursor_y;
        self.state.cursor_x = x;
        self.state.cursor_y = y;

        let new_hover = render::hit_test_toolbar_buttons(x, y, _width);
        let hover_dirty = new_hover != self.state.toolbar_btn_hovered;
        if hover_dirty {
            self.state.toolbar_btn_hovered = new_hover;
        }

        if self.state.is_dragging {
            let dx = prev_x - x;
            let dy = prev_y - y;
            let num_users = plan.users_data.len();
            let max_y = Self::max_scroll_y(num_users, height);
            self.state.scroll_x += dx;
            self.state.scroll_y = (self.state.scroll_y + dy).clamp(0.0, max_y);
            let alpha = 0.4_f32;
            self.state.drag_vel_x = self.state.drag_vel_x * (1.0 - alpha) + dx * alpha;
            self.state.drag_vel_y = self.state.drag_vel_y * (1.0 - alpha) + dy * alpha;
            return DirtyRegion::PageOnly;
        }

        if hover_dirty {
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
        _height: f32,
        plan: &Plan,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        let is_content = y > TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE;

        if pressed {
            if is_content {
                self.state.is_dragging = true;
                self.state.last_drag_x = x;
                self.state.last_drag_y = y;
                self.state.press_start_x = x;
                self.state.press_start_y = y;
                self.state.drag_vel_x = 0.0;
                self.state.drag_vel_y = 0.0;
                self.state.vel_x = 0.0;
                self.state.vel_y = 0.0;
            } else {
                match render::hit_test_toolbar_buttons(x, y, width) {
                    Some(0) => {
                        // Today: scroll to today
                        use chrono::Local;
                        let today = Local::now().date_naive();
                        let days = (today - plan.start_date).num_days();
                        self.state.scroll_x = days as f32 * self.state.zoom - (width * 0.5);
                        self.state.vel_x = 0.0;
                    }
                    Some(1) => {
                        self.state.open_users_window = true;
                    }
                    Some(2) => {
                        self.state.settings_init_name = plan.name.clone();
                        self.state.settings_init_date = plan.start_date.to_string();
                        self.state.settings_init_scheduler_target = plan.scheduler_target;
                        self.state.open_settings_window = true;
                    }
                    _ => {}
                }
            }
        } else if self.state.is_dragging {
            self.state.is_dragging = false;
            let speed = (self.state.drag_vel_x.powi(2) + self.state.drag_vel_y.powi(2)).sqrt();
            if speed > 1.5 {
                self.state.vel_x = self.state.drag_vel_x * 3.0;
                self.state.vel_y = self.state.drag_vel_y * 3.0;
            }
        }
        DirtyRegion::PageOnly
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        shift: bool,
        _width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        if shift {
            let factor = if delta_y > 0.0 {
                1.025_f32
            } else {
                1.0 / 1.025
            };
            self.state.zoom_target =
                (self.state.zoom_target * factor).clamp(GANTT_ZOOM_MIN, GANTT_ZOOM_MAX);
        } else {
            self.state.vel_y -= delta_y * 4.0;
            self.state.vel_y = self.state.vel_y.clamp(-300.0, 300.0);
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
    }

    fn has_animation(&self) -> bool {
        self.state.vel_x.abs() > 0.1
            || self.state.vel_y.abs() > 0.1
            || (self.state.zoom_target - self.state.zoom).abs() > 0.05
    }

    fn tick_animation(&mut self, width: f32, height: f32, plan: &Plan) -> DirtyRegion {
        let friction = 0.88_f32;
        let mut dirty = false;

        let zoom_diff = self.state.zoom_target - self.state.zoom;
        if zoom_diff.abs() > 0.01 {
            let old_zoom = self.state.zoom;
            self.state.zoom += zoom_diff * 0.18;
            let new_zoom = self.state.zoom;
            let ratio = new_zoom / old_zoom;
            let pivot_x = if self.state.cursor_x >= 0.0 && self.state.cursor_x <= width {
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
        if self.state.vel_y.abs() > 0.1 {
            let max = Self::max_scroll_y(plan.users_data.len(), height);
            self.state.scroll_y = (self.state.scroll_y + self.state.vel_y).clamp(0.0, max);
            self.state.vel_y *= friction;
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
