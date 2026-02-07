use skia_safe::{Canvas, Color, Paint, PaintStyle, Rect, TextBlob};

use crate::ui::cache::RenderCache;
use crate::ui::layout::*;

use super::state::PlanningState;

pub fn draw_planning(canvas: &Canvas, x: f32, y: f32, w: f32, h: f32, state: &PlanningState, cache: &RenderCache) {
    let divider_x = x + w * state.divider_ratio;

    let left_width = w * state.divider_ratio - DIVIDER_WIDTH / 2.0;
    draw_panel(canvas, x, y, left_width, h, &cache.left_panel_label);

    let right_x = divider_x + DIVIDER_WIDTH / 2.0;
    let right_width = w - (right_x - x);
    draw_panel(canvas, right_x, y, right_width, h, &cache.right_panel_label);

    let active = state.dragging_divider || state.hovering_divider;
    draw_divider(canvas, divider_x, y, y + h, active);
}

fn draw_panel(
    canvas: &Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &TextBlob,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(PANEL_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(x, y, w, h), &paint);

    paint.set_color(Color::from(PANEL_TEXT));
    let bounds = label.bounds();
    let tx = x + (w - bounds.width()) / 2.0 - bounds.left();
    let ty = y + (h - bounds.height()) / 2.0 - bounds.top();
    canvas.draw_text_blob(label, (tx, ty), &paint);
}

fn draw_divider(canvas: &Canvas, x: f32, top: f32, bottom: f32, active: bool) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    let color = if active {
        DIVIDER_ACTIVE_COLOR
    } else {
        DIVIDER_COLOR
    };
    paint.set_color(Color::from(color));
    canvas.draw_rect(
        Rect::from_xywh(x - DIVIDER_WIDTH / 2.0, top, DIVIDER_WIDTH, bottom - top),
        &paint,
    );

    let grip_color = if active {
        DIVIDER_GRIP_ACTIVE_COLOR
    } else {
        DIVIDER_GRIP_COLOR
    };
    paint.set_color(Color::from(grip_color));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);

    let cy = (top + bottom) / 2.0;
    let grip_w = DIVIDER_WIDTH * 0.5;
    for i in -1..=1 {
        let gy = cy + i as f32 * 3.0;
        canvas.draw_line((x - grip_w / 2.0, gy), (x + grip_w / 2.0, gy), &paint);
    }
}

pub fn hit_test_divider(x: f32, page_width: f32, divider_ratio: f32) -> bool {
    let divider_x = page_width * divider_ratio;
    let half = DIVIDER_WIDTH / 2.0 + 2.0;
    x >= divider_x - half && x <= divider_x + half
}
