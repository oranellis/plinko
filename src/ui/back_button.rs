//! Drawing and hit-testing for the back-navigation button shown on all pages.

use skia_safe::{Canvas, Color, Paint, PaintStyle, PathBuilder, RRect, Rect};

use super::icon_button::{begin_blur_backdrop, end_blur_backdrop};
use super::layout::*;

/// Draws the back button at the fixed position defined in [`layout`](super::layout).
///
/// Renders a blurred backdrop, a rounded-rect hover background when `hovered`
/// is `true`, and always renders a left-pointing chevron arrow.
pub fn draw_back_button(canvas: &Canvas, hovered: bool) {
    begin_blur_backdrop(canvas, BACK_BTN_X, BACK_BTN_Y);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    if hovered {
        paint.set_color(Color::from(BACK_BTN_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        let rrect = RRect::new_rect_xy(
            Rect::from_xywh(BACK_BTN_X, BACK_BTN_Y, BACK_BTN_SIZE, BACK_BTN_SIZE),
            BACK_BTN_CORNER,
            BACK_BTN_CORNER,
        );
        canvas.draw_rrect(rrect, &paint);
    }

    // Left-pointing chevron
    let cx = BACK_BTN_X + BACK_BTN_SIZE / 2.0;
    let cy = BACK_BTN_Y + BACK_BTN_SIZE / 2.0;
    let aw = BACK_BTN_SIZE * 0.3;
    let ah = BACK_BTN_SIZE * 0.3;

    let mut pb = PathBuilder::new();
    pb.move_to((cx + aw / 2.0, cy - ah / 2.0));
    pb.line_to((cx - aw / 2.0, cy));
    pb.line_to((cx + aw / 2.0, cy + ah / 2.0));
    let path = pb.detach();

    paint.set_color(Color::from(BACK_BTN_ICON_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
    canvas.draw_path(&path, &paint);

    end_blur_backdrop(canvas, BACK_BTN_X, BACK_BTN_Y);
}

/// Returns `true` if the logical cursor position `(x, y)` is inside the back
/// button's bounding rectangle.
pub fn hit_test_back_button(x: f32, y: f32) -> bool {
    (BACK_BTN_X..=BACK_BTN_X + BACK_BTN_SIZE).contains(&x)
        && (BACK_BTN_Y..=BACK_BTN_Y + BACK_BTN_SIZE).contains(&y)
}
