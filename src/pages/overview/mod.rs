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
use crate::ui::layout::{GANTT_ROW_H, GANTT_ZOOM_MAX, GANTT_ZOOM_MIN};
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
        use crate::ui::layout::{GANTT_HEADER_H, TOOLBAR_BTN_SIZE, TOOLBAR_BTN_Y};
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
        _width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        self.state.cursor_x = x;
        self.state.cursor_y = y;
        let new_hover = render::hit_test_toolbar_buttons(x, y);
        if new_hover != self.state.toolbar_btn_hovered {
            self.state.toolbar_btn_hovered = new_hover;
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
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        if pressed {
            match render::hit_test_toolbar_buttons(x, y) {
                Some(0) => self.state.open_users_window = true,
                Some(1) => self.state.open_task_form = true,
                Some(2) => self.state.open_milestone_form = true,
                _ => {}
            }
        }
        DirtyRegion::None
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        shift: bool,
        _width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion {
        if shift {
            // Shift+scroll → zoom
            let factor = if delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
            let new_zoom = (self.state.zoom * factor).clamp(GANTT_ZOOM_MIN, GANTT_ZOOM_MAX);
            if (new_zoom - self.state.zoom).abs() > f32::EPSILON {
                self.state.zoom = new_zoom;
                return DirtyRegion::PageOnly;
            }
        } else {
            // Normal scroll → vertical scroll
            let rows = gantt::pack_rows(plan);
            let max = Self::max_scroll_y(rows.len(), height);
            let new_scroll = (self.state.scroll_y - delta_y * 40.0).clamp(0.0, max);
            if (new_scroll - self.state.scroll_y).abs() > f32::EPSILON {
                self.state.scroll_y = new_scroll;
                return DirtyRegion::PageOnly;
            }
        }
        DirtyRegion::None
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
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
        None
    }

    fn reset_hover(&mut self) {
        self.state.toolbar_btn_hovered = None;
    }
}
