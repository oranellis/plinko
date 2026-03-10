//! Generic icon-only toolbar button: no visible background or border at rest,
//! rounded-rect highlight on hover.  A blurred backdrop is always applied so
//! the icon remains readable over page content underneath.

use skia_safe::{
    Canvas, ClipOp, Color, Matrix, Paint, PaintStyle, Path, RRect, Rect, canvas::SaveLayerRec,
    image_filters,
};

use super::layout::*;

/// Gaussian sigma for the backdrop blur behind every toolbar button.
pub const BLUR_SIGMA: f32 = 1.3;

/// How many pixels the blur region extends beyond the button edge.
/// Kept small so adjacent toolbar buttons (gap = TOOLBAR_BTN_GAP = 4 px) do
/// not overlap each other's blur regions.
const CLIP_EXPAND: f32 = 1.0;

/// Opens a clipped backdrop-blur layer for a toolbar button at `(x, y)`.
///
/// Must be paired with a call to [`end_blur_backdrop`].  All content drawn
/// between the two calls appears on top of the blurred background.
///
/// Use this in every toolbar button draw function (back button, icon buttons,
/// etc.) so the blur behaviour is consistent across the whole toolbar.
pub fn begin_blur_backdrop(canvas: &Canvas, x: f32, y: f32) {
    let corner = TOOLBAR_BTN_CORNER + CLIP_EXPAND;
    let clip_rrect = RRect::new_rect_xy(
        Rect::from_xywh(
            x - CLIP_EXPAND,
            y - CLIP_EXPAND,
            TOOLBAR_BTN_SIZE + 2.0 * CLIP_EXPAND,
            TOOLBAR_BTN_SIZE + 2.0 * CLIP_EXPAND,
        ),
        corner,
        corner,
    );

    canvas.save();
    canvas.clip_rrect(clip_rrect, ClipOp::Intersect, true);

    if let Some(filter) = image_filters::blur((BLUR_SIGMA, BLUR_SIGMA), None, None, None) {
        canvas.save_layer(&SaveLayerRec::default().backdrop(&filter));
    } else {
        canvas.save();
    }
}

/// Closes the backdrop-blur layer opened by [`begin_blur_backdrop`].
pub fn end_blur_backdrop(canvas: &Canvas, x: f32, y: f32) {
    let _ = (x, y);
    canvas.restore(); // backdrop layer (or inner save)
    canvas.restore(); // clip
}

/// Draws a single icon button at `(x, y)` with size `TOOLBAR_BTN_SIZE`.
///
/// A blurred-backdrop layer is always opened so the icon is legible over any
/// page content drawn beneath it.  A rounded-rect hover highlight is drawn
/// inside the layer when `hovered` is `true`.
///
/// The `icon` path is assumed to fill an arbitrary bounding box; it is scaled
/// and centred to ~50 % of the button size.
pub fn draw_icon_button(canvas: &Canvas, x: f32, y: f32, hovered: bool, icon: &Path) {
    let btn_rect = Rect::from_xywh(x, y, TOOLBAR_BTN_SIZE, TOOLBAR_BTN_SIZE);

    begin_blur_backdrop(canvas, x, y);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    if hovered {
        paint.set_color(Color::from(TOOLBAR_BTN_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(btn_rect, TOOLBAR_BTN_CORNER, TOOLBAR_BTN_CORNER),
            &paint,
        );
    }

    // Scale icon to ~50% of button size and centre it.
    let icon_draw_size = TOOLBAR_BTN_SIZE * 0.5;
    let offset_x = x + (TOOLBAR_BTN_SIZE - icon_draw_size) / 2.0;
    let offset_y = y + (TOOLBAR_BTN_SIZE - icon_draw_size) / 2.0;

    let icon_bounds = icon.bounds();
    let src_w = icon_bounds.width().max(1.0);
    let src_h = icon_bounds.height().max(1.0);
    let scale_x = icon_draw_size / src_w;
    let scale_y = icon_draw_size / src_h;
    let tx = offset_x - icon_bounds.left * scale_x;
    let ty = offset_y - icon_bounds.top * scale_y;

    let matrix = Matrix::scale_translate((scale_x, scale_y), (tx, ty));
    let scaled = icon.with_transform(&matrix);

    paint.set_color(Color::from(TOOLBAR_BTN_ICON_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(2.0 / scale_x.min(scale_y));
    canvas.draw_path(&scaled, &paint);

    end_blur_backdrop(canvas, x, y);
}

/// Returns `true` if `(px, py)` falls inside the toolbar button at `(x, y)`.
pub fn hit_test_icon_button(px: f32, py: f32, x: f32, y: f32) -> bool {
    (x..=x + TOOLBAR_BTN_SIZE).contains(&px) && (y..=y + TOOLBAR_BTN_SIZE).contains(&py)
}
