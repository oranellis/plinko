//! Shared drawing helpers for the standard floating-window chrome.
//!
//! All floating windows should use these functions to guarantee consistent:
//! - drop shadow
//! - panel background
//! - title bar (LIST_BG with rounded top corners, ITEM_FG title text)
//! - left-chevron back button
//! - 1-px divider below title bar
//! - proportional scrollbar thumb

use skia_safe::{Canvas, Color, Paint, PaintStyle, PathBuilder, RRect, Rect, TextBlob};

use crate::ui::cache::RenderCache;
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, DIVIDER_COLOR, ITEM_FG,
    LIST_BG, OVERLAY_SOFT, PANEL_BG, SCROLLBAR_THUMB_COLOR, TOOLBAR_STROKE_WIDTH,
};

pub fn draw_chevron_btn(canvas: &Canvas, btn_rect: Rect, hovered: bool) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if hovered {
        paint.set_color(Color::from(BACK_BTN_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(btn_rect, BACK_BTN_CORNER, BACK_BTN_CORNER),
            &paint,
        );
    }
    let cx = btn_rect.left + BACK_BTN_SIZE / 2.0;
    let cy = btn_rect.top + BACK_BTN_SIZE / 2.0;
    let aw = BACK_BTN_SIZE * 0.3;
    let ah = BACK_BTN_SIZE * 0.3;
    let mut pb = PathBuilder::new();
    pb.move_to((cx + aw / 2.0, cy - ah / 2.0));
    pb.line_to((cx - aw / 2.0, cy));
    pb.line_to((cx + aw / 2.0, cy + ah / 2.0));
    paint.set_color(Color::from(BACK_BTN_ICON_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
    canvas.draw_path(&pb.detach(), &paint);
}

/// Draws the complete standard window chrome: shadow, panel background, title
/// bar with rounded top corners, centred title text, chevron back button, and
/// 1-px divider below the title bar.
pub fn draw_window_chrome(
    canvas: &Canvas,
    panel: Rect,
    corner: f32,
    title_h: f32,
    title: &str,
    hovered_back: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Drop shadow
    paint.set_color(Color::from(OVERLAY_SOFT));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(
                panel.left + 2.0,
                panel.top + 4.0,
                panel.width(),
                panel.height(),
            ),
            corner,
            corner,
        ),
        &paint,
    );

    // Panel background
    paint.set_color(Color::from(PANEL_BG));
    canvas.draw_rrect(RRect::new_rect_xy(panel, corner, corner), &paint);

    // Title bar background (rounded top corners only)
    let title_rect = Rect::from_xywh(panel.left, panel.top, panel.width(), title_h);
    paint.set_color(Color::from(LIST_BG));
    canvas.draw_rrect(RRect::new_rect_xy(title_rect, corner, corner), &paint);
    canvas.draw_rect(
        Rect::from_xywh(
            panel.left,
            panel.top + corner,
            panel.width(),
            title_h - corner,
        ),
        &paint,
    );

    // Title text (centred)
    paint.set_color(Color::from(ITEM_FG));
    if let Some(blob) = TextBlob::new(title, &cache.font) {
        let (adv, _) = cache.font.measure_str(title, None);
        let (_, m) = cache.font.metrics();
        let tx = panel.left + (panel.width() - adv) / 2.0;
        let ty = panel.top + (title_h - (m.descent - m.ascent)) / 2.0 - m.ascent;
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    // Back chevron button
    let btn_rect = Rect::from_xywh(
        panel.left + (title_h - BACK_BTN_SIZE) / 2.0,
        panel.top + (title_h - BACK_BTN_SIZE) / 2.0,
        BACK_BTN_SIZE,
        BACK_BTN_SIZE,
    );
    draw_chevron_btn(canvas, btn_rect, hovered_back);

    // Divider below title bar
    paint.set_color(Color::from(DIVIDER_COLOR));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(panel.left, panel.top + title_h, panel.width(), 1.0),
        &paint,
    );
}

/// Draws a proportional scrollbar thumb on the right edge of the panel.
/// Does nothing if `total_h <= visible_h`.
pub fn draw_window_scrollbar(
    canvas: &Canvas,
    panel_right: f32,
    content_top: f32,
    visible_h: f32,
    total_h: f32,
    scroll_y: f32,
) {
    if total_h <= visible_h {
        return;
    }
    const SCROLLBAR_W: f32 = 4.0;
    let max_scroll = total_h - visible_h;
    let thumb_h = (visible_h * visible_h / total_h).max(20.0);
    let thumb_y = content_top + (scroll_y / max_scroll) * (visible_h - thumb_h);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(
                panel_right - SCROLLBAR_W - 2.0,
                thumb_y,
                SCROLLBAR_W,
                thumb_h,
            ),
            2.0,
            2.0,
        ),
        &paint,
    );
}
