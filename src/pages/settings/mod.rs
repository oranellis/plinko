//! Settings page — placeholder for application configuration UI.

pub mod render;
pub mod state;

use skia_safe::Canvas;

use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

/// Settings page stub.  Renders a centred label; full settings UI is not yet implemented.
pub struct SettingsPage {
    #[allow(dead_code)]
    pub state: state::SettingsState,
}

impl SettingsPage {
    pub fn new() -> Self {
        Self {
            state: state::SettingsState,
        }
    }
}

impl Page for SettingsPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache) {
        render::draw_settings(canvas, 0.0, 0.0, width, height, cache);
    }

    fn on_cursor_moved(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) -> DirtyRegion {
        DirtyRegion::None
    }

    fn on_mouse_input(&mut self, _x: f32, _y: f32, _pressed: bool, _width: f32, _height: f32) -> DirtyRegion {
        DirtyRegion::None
    }
}
