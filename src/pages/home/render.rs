//! Rendering functions for the home page.

use skia_safe::{Canvas, Color, Paint, PaintStyle, RRect, Rect};

use crate::ui::cache::RenderCache;
use crate::ui::layout::*;

/// Computes the bounding rectangles for five navigation cards:
/// row 1 (top): Daily (0), Overview (1), Settings (2)
/// row 2 (bottom, centered): Allocation (3), Calendar (4)
fn card_rects(width: f32, height: f32) -> [Rect; 5] {
    let row1_w = 3.0 * HOME_CARD_SIZE + 2.0 * HOME_CARD_GAP;
    let row2_w = 2.0 * HOME_CARD_SIZE + HOME_CARD_GAP;
    let total_h = 2.0 * HOME_CARD_SIZE + HOME_CARD_GAP;
    let start_y = (height - total_h) / 2.0;

    let row1_x = (width - row1_w) / 2.0;
    let row2_x = (width - row2_w) / 2.0;
    let row2_y = start_y + HOME_CARD_SIZE + HOME_CARD_GAP;

    [
        Rect::from_xywh(row1_x, start_y, HOME_CARD_SIZE, HOME_CARD_SIZE),
        Rect::from_xywh(
            row1_x + HOME_CARD_SIZE + HOME_CARD_GAP,
            start_y,
            HOME_CARD_SIZE,
            HOME_CARD_SIZE,
        ),
        Rect::from_xywh(
            row1_x + 2.0 * (HOME_CARD_SIZE + HOME_CARD_GAP),
            start_y,
            HOME_CARD_SIZE,
            HOME_CARD_SIZE,
        ),
        Rect::from_xywh(row2_x, row2_y, HOME_CARD_SIZE, HOME_CARD_SIZE),
        Rect::from_xywh(
            row2_x + HOME_CARD_SIZE + HOME_CARD_GAP,
            row2_y,
            HOME_CARD_SIZE,
            HOME_CARD_SIZE,
        ),
    ]
}

/// Draws the full home screen: background fill, five navigation cards each
/// with a hover state, a pre-built icon path, and a centred label.
pub fn draw_home(
    canvas: &Canvas,
    width: f32,
    height: f32,
    hovered_card: Option<usize>,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Background
    paint.set_color(Color::from(HOME_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, width, height), &paint);

    let rects = card_rects(width, height);
    for (i, rect) in rects.iter().enumerate() {
        // Card background
        let bg = if hovered_card == Some(i) {
            HOME_CARD_HOVER_BG
        } else {
            HOME_CARD_BG
        };
        let rrect = RRect::new_rect_xy(*rect, HOME_CARD_CORNER, HOME_CARD_CORNER);

        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);

        // Card border
        paint.set_color(Color::from(HOME_CARD_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(rrect, &paint);

        // Icon centered in upper portion
        let icon_x = rect.left() + (HOME_CARD_SIZE - HOME_CARD_ICON_SIZE) / 2.0;
        let icon_y = rect.top() + HOME_CARD_SIZE * 0.2;
        paint.set_color(Color::from(HOME_ICON_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.save();
        canvas.translate((icon_x, icon_y));
        canvas.draw_path(&cache.home_icon_paths[i], &paint);
        canvas.restore();

        // Label centered below icon
        let label = &cache.home_card_labels[i];
        let bounds = label.bounds();
        let lx = rect.left() + (HOME_CARD_SIZE - bounds.width()) / 2.0 - bounds.left();
        let ly = rect.top() + HOME_CARD_SIZE * 0.78 - bounds.top();
        paint.set_color(Color::from(HOME_CARD_LABEL_COLOR));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_text_blob(label, (lx, ly), &paint);
    }
}

/// Returns the index of the card under logical cursor position `(x, y)`,
/// or `None` if the cursor is not over any card.
pub fn hit_test_card(x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
    let rects = card_rects(width, height);
    for (i, rect) in rects.iter().enumerate() {
        if x >= rect.left() && x <= rect.right() && y >= rect.top() && y <= rect.bottom() {
            return Some(i);
        }
    }
    None
}
