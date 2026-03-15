//! Overview page — full-window Gantt chart view.

pub mod gantt;
pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::data::Plan;
use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
use crate::ui::layout::{
    GANTT_HEADER_H, GANTT_ROW_H, GANTT_ZOOM_DEFAULT, GANTT_ZOOM_MAX, GANTT_ZOOM_MIN,
    TOOLBAR_BTN_SIZE, TOOLBAR_BTN_Y,
};
use crate::ui::milestone_form_window::MilestoneFormWindow;
use crate::ui::task_form_window::TaskFormWindow;
use crate::ui::users_window::UsersWindow;

use state::OverviewState;

/// Overview page: full-window Gantt chart.
pub struct OverviewPage {
    pub state: OverviewState,
}

impl OverviewPage {
    pub fn new() -> Self {
        Self {
            state: OverviewState::new(),
        }
    }

    /// Y coordinate where Gantt rows start.
    fn gantt_rows_top() -> f32 {
        use crate::ui::layout::GANTT_HEADER_H;
        TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0 + GANTT_HEADER_H
    }

    /// Maximum vertical scroll given the number of rows and visible height.
    fn max_scroll_y(rows: usize, height: f32) -> f32 {
        let content_h = rows as f32 * GANTT_ROW_H;
        let visible_h = height - Self::gantt_rows_top();
        (content_h - visible_h).max(0.0)
    }
}

impl Page for OverviewPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        render::draw_overview(canvas, 0.0, 0.0, width, height, &self.state, cache, plan);
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
        let hover_dirty = new_hover != self.state.toolbar_btn_hovered;
        if hover_dirty {
            self.state.toolbar_btn_hovered = new_hover;
        }

        if self.state.is_dragging {
            let dx = prev_x - x;
            let dy = prev_y - y;

            let rows = gantt::pack_rows(plan);
            let max_y = Self::max_scroll_y(rows.len(), height);
            // No .max(0.0) — allow panning left indefinitely
            self.state.scroll_x += dx;
            self.state.scroll_y = (self.state.scroll_y + dy).clamp(0.0, max_y);

            let alpha = 0.4_f32;
            self.state.drag_vel_x = self.state.drag_vel_x * (1.0 - alpha) + dx * alpha;
            self.state.drag_vel_y = self.state.drag_vel_y * (1.0 - alpha) + dy * alpha;

            self.state.last_drag_x = x;
            self.state.last_drag_y = y;

            return DirtyRegion::PageOnly;
        }

        // Hit-test warning icons; update hovered_warning.
        let rows = gantt::pack_rows(plan);
        let view_start = render::view_start_date(plan);
        let prev_warning = self.state.hovered_warning;
        self.state.hovered_warning =
            render::hit_test_warning_icon(x, y, plan, &rows, &self.state, height, view_start);
        let warning_dirty = self.state.hovered_warning != prev_warning;

        if hover_dirty || warning_dirty {
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
        let is_gantt_area = y > TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE;

        if pressed {
            if is_gantt_area {
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
                        // Today button: centre the Gantt view on today's date
                        use chrono::Local;
                        use gantt::compute_date_range;
                        let today = Local::now().date_naive();
                        let view_start = compute_date_range(plan)
                            .map(|(s, _)| s)
                            .unwrap_or(plan.start_date);
                        let days = (today - view_start).num_days();
                        self.state.scroll_x = days as f32 * self.state.zoom - (width * 0.5);
                        self.state.vel_x = 0.0;
                        self.state.vel_y = 0.0;
                    }
                    Some(1) => self.state.open_task_form = true,
                    Some(2) => self.state.open_milestone_form = true,
                    Some(3) => self.state.open_users_window = true,
                    Some(4) => {
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

            let drag_dist = ((x - self.state.press_start_x).powi(2)
                + (y - self.state.press_start_y).powi(2))
            .sqrt();

            if drag_dist < 6.0 {
                // Short drag / click — hit-test for task or milestone
                use gantt::{compute_date_range, pack_rows};
                let rows = pack_rows(plan);
                let view_start = compute_date_range(plan)
                    .map(|(s, _)| s)
                    .unwrap_or(plan.start_date);
                if let Some(hit) =
                    render::hit_test_gantt_item(x, y, &rows, &self.state, height, view_start)
                {
                    match hit {
                        render::GanttHit::Task(id) => {
                            if let Some(task) = plan.tasks.get(&id) {
                                self.state.pending_window =
                                    Some(Box::new(TaskFormWindow::from_task(task)));
                            }
                        }
                        render::GanttHit::Milestone(id) => {
                            if let Some(ms) = plan.milestones.get(&id) {
                                self.state.pending_window =
                                    Some(Box::new(MilestoneFormWindow::from_milestone(ms)));
                            }
                        }
                    }
                }
            } else {
                let speed = (self.state.drag_vel_x.powi(2) + self.state.drag_vel_y.powi(2)).sqrt();
                if speed > 1.5 {
                    self.state.vel_x = self.state.drag_vel_x * 3.0;
                    self.state.vel_y = self.state.drag_vel_y * 3.0;
                }
            }
        }
        // Any press or release may have mutated scroll / opened a window — always redraw.
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
            // Nudge zoom target; tick_animation lerps toward it smoothly.
            let factor = if delta_y > 0.0 {
                1.025_f32
            } else {
                1.0 / 1.025
            };
            self.state.zoom_target =
                (self.state.zoom_target * factor).clamp(GANTT_ZOOM_MIN, GANTT_ZOOM_MAX);
            DirtyRegion::PageOnly
        } else {
            self.state.vel_y -= delta_y * 4.0;
            self.state.vel_y = self.state.vel_y.clamp(-300.0, 300.0);
            DirtyRegion::PageOnly
        }
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if let Some(w) = self.state.pending_window.take() {
            return Some(w);
        }
        if self.state.open_users_window {
            self.state.open_users_window = false;
            return Some(Box::new(UsersWindow::new()));
        }
        if self.state.open_task_form {
            self.state.open_task_form = false;
            return Some(Box::new(TaskFormWindow::new()));
        }
        if self.state.open_milestone_form {
            self.state.open_milestone_form = false;
            return Some(Box::new(MilestoneFormWindow::new()));
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
        self.state.hovered_warning = None;
    }

    fn has_animation(&self) -> bool {
        self.state.vel_x.abs() > 0.1
            || self.state.vel_y.abs() > 0.1
            || (self.state.zoom_target - self.state.zoom).abs() > 0.05
    }

    fn tick_animation(&mut self, width: f32, height: f32, plan: &Plan) -> DirtyRegion {
        let friction = 0.88_f32;
        let mut dirty = false;

        // Smooth zoom: lerp toward zoom_target, pivoting around the cursor.
        let zoom_diff = self.state.zoom_target - self.state.zoom;
        if zoom_diff.abs() > 0.01 {
            let old_zoom = self.state.zoom;
            self.state.zoom += zoom_diff * 0.18;
            let new_zoom = self.state.zoom;
            let ratio = new_zoom / old_zoom;
            // Use cursor_x as the pivot; fall back to screen centre if off-screen.
            let pivot_x = if self.state.cursor_x >= 0.0 && self.state.cursor_x <= width {
                self.state.cursor_x
            } else {
                width * 0.5
            };
            // Keep the date under pivot_x stationary:
            // scroll_x_new = (pivot_x + scroll_x_old) * ratio - pivot_x
            self.state.scroll_x = (pivot_x + self.state.scroll_x) * ratio - pivot_x;
            dirty = true;
        } else if zoom_diff.abs() > f32::EPSILON {
            self.state.zoom = self.state.zoom_target;
        }

        if self.state.vel_x.abs() > 0.1 {
            self.state.scroll_x += self.state.vel_x; // no left clamp — allow infinite left pan
            self.state.vel_x *= friction;
            dirty = true;
        }
        if self.state.vel_y.abs() > 0.1 {
            let rows = gantt::pack_rows(plan);
            let max = Self::max_scroll_y(rows.len(), height);
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
