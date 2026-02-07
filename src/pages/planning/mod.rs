pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

pub struct PlanningPage {
    pub state: state::PlanningState,
}

impl PlanningPage {
    pub fn new() -> Self {
        Self {
            state: state::PlanningState::new(),
        }
    }
}

impl Page for PlanningPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache) {
        render::draw_planning(canvas, 0.0, 0.0, width, height, &self.state, cache);
    }

    fn on_cursor_moved(&mut self, x: f32, _y: f32, width: f32, _height: f32) -> DirtyRegion {
        if self.state.dragging_divider {
            if width > 0.0 {
                self.state.divider_ratio = (x / width).clamp(0.1, 0.9);
            }
            return DirtyRegion::PageOnly;
        }

        let over_divider = render::hit_test_divider(x, width, self.state.divider_ratio);
        if over_divider != self.state.hovering_divider {
            self.state.hovering_divider = over_divider;
            return DirtyRegion::PageOnly;
        }

        DirtyRegion::None
    }

    fn on_mouse_input(&mut self, x: f32, _y: f32, pressed: bool, width: f32, _height: f32) -> DirtyRegion {
        if pressed {
            if render::hit_test_divider(x, width, self.state.divider_ratio) {
                self.state.dragging_divider = true;
                return DirtyRegion::PageOnly;
            }
        } else if self.state.dragging_divider {
            self.state.dragging_divider = false;
            return DirtyRegion::PageOnly;
        }
        DirtyRegion::None
    }
}
