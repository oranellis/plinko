//! Rendering functions for the settings page.

use skia_safe::{Canvas, Color, Paint, PaintStyle, Rect};

use crate::ui::cache::RenderCache;
use crate::ui::layout::{PANEL_BG, PANEL_TEXT};

/// Fills the panel area with `PANEL_BG` and draws a centred "Settings" label.
/// `(x, y)` is the top-left origin; `(w, h)` are the logical dimensions.
pub fn draw_settings(canvas: &Canvas, x: f32, y: f32, w: f32, h: f32, cache: &RenderCache) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(PANEL_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(x, y, w, h), &paint);

    paint.set_color(Color::from(PANEL_TEXT));
    let blob = &cache.settings_label;
    let bounds = blob.bounds();
    let tx = x + (w - bounds.width()) / 2.0 - bounds.left();
    let ty = y + (h - bounds.height()) / 2.0 - bounds.top();
    canvas.draw_text_blob(blob, (tx, ty), &paint);
}
