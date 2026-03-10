//! Overview page — full-window Gantt chart view.

pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::data::Plan;
use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
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
        _plan: &Plan,
    ) -> DirtyRegion {
        let _ = (width, height);
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
        if pressed && let Some(0) = render::hit_test_toolbar_buttons(x, y) {
            self.state.open_users_window = true;
        }
        DirtyRegion::None
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if self.state.open_users_window {
            self.state.open_users_window = false;
            Some(Box::new(UsersWindow::new()))
        } else {
            None
        }
    }

    fn reset_hover(&mut self) {
        self.state.toolbar_btn_hovered = None;
    }
}
