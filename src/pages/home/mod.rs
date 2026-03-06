pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

pub struct HomePage {
    pub state: state::HomeState,
}

impl HomePage {
    pub fn new() -> Self {
        Self {
            state: state::HomeState::new(),
        }
    }
}

impl Page for HomePage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache) {
        render::draw_home(canvas, width, height, self.state.hovered_card, cache);
    }

    fn on_cursor_moved(&mut self, x: f32, y: f32, width: f32, height: f32) -> DirtyRegion {
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
    ) -> DirtyRegion {
        DirtyRegion::None
    }
}
