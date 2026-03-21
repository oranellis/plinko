//! Home page — card grid that lets the user navigate to Daily, Overview, or Settings.

pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::data::Plan;
use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

/// Home page: shows a row of navigation cards centred on screen.
pub struct HomePage {
    pub state: state::HomeState,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl HomePage {
    pub fn new() -> Self {
        Self {
            state: state::HomeState::new(),
        }
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Page for HomePage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        render::draw_home(canvas, width, height, self.state.hovered_card, cache);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        let new_hovered = render::hit_test_card(x, y, width, height);
        if new_hovered != self.state.hovered_card {
            self.state.hovered_card = new_hovered;
            DirtyRegion::PageOnly
        } else {
            DirtyRegion::None
        }
    }

    fn on_mouse_input(
        &mut self,
        _x: f32,
        _y: f32,
        _pressed: bool,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        DirtyRegion::None
    }

    fn reset_hover(&mut self) {
        self.state.hovered_card = None;
    }
}
// }}}
