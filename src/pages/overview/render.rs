//! Rendering functions for the overview page.

use skia_safe::Canvas;

use crate::data::Plan;
use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;

use super::state::OverviewState;

/// Draws the overview page.
#[allow(clippy::too_many_arguments)]
pub fn draw_overview(
    canvas: &Canvas,
    _x: f32,
    _y: f32,
    _w: f32,
    _h: f32,
    state: &OverviewState,
    cache: &RenderCache,
    _plan: &Plan,
) {
    draw_toolbar_buttons(canvas, state, cache);
}

fn draw_toolbar_buttons(canvas: &Canvas, state: &OverviewState, cache: &RenderCache) {
    // 0 — person (placeholder: team / users)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(0),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(0),
        &cache.icon_person,
    );
    // 1 — plus (placeholder: add task)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(1),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(1),
        &cache.icon_plus,
    );
    // 2 — diamond (placeholder: add milestone)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(2),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(2),
        &cache.icon_diamond,
    );
}

/// Returns the index of the hovered page toolbar button, or `None`.
pub fn hit_test_toolbar_buttons(px: f32, py: f32) -> Option<usize> {
    for i in 0..3_u32 {
        if icon_button::hit_test_icon_button(px, py, toolbar_btn_x(i), TOOLBAR_BTN_Y) {
            return Some(i as usize);
        }
    }
    None
}
