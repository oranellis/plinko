//! A lightweight floating error notification shown when the server rejects a
//! plan mutation (e.g. a constraint date that makes the schedule impossible).

use skia_safe::{Canvas, Color, Contains, Font, Paint, PaintStyle, Point, RRect, Rect, TextBlob};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BTN_PRIMARY_BG, BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, ERROR_BG, INPUT_BORDER_ERROR, ITEM_FG,
    MUTED_FG, PANEL_BG, PLAN_FORM_PADDING,
};
use plinko_shared::data::Plan;

const PANEL_W: f32 = 420.0;
const CORNER: f32 = 8.0;
const TITLE_H: f32 = 52.0;
const PAD: f32 = PLAN_FORM_PADDING;
const DISMISS_BTN_H: f32 = 36.0;
const DISMISS_BTN_W: f32 = 100.0;
const LINE_GAP: f32 = 2.0;

fn wrap_text(text: &str, font: &Font, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        let (w, _) = font.measure_str(&candidate, None);
        if w > max_w && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

pub struct ErrorWindow {
    reason: String,
    hovered_dismiss: bool,
    dismiss_rect: Rect,
}

impl ErrorWindow {
    pub fn new(reason: String) -> Self {
        Self {
            reason,
            hovered_dismiss: false,
            dismiss_rect: Rect::default(),
        }
    }

    fn panel_rect(width: f32, height: f32, panel_h: f32) -> Rect {
        let x = (width - PANEL_W) / 2.0;
        let y = (height - panel_h) / 2.0;
        Rect::from_xywh(x, y, PANEL_W, panel_h)
    }

    fn compute_panel_h(&self, cache: &RenderCache) -> f32 {
        let (_, bm) = cache.small_font.metrics();
        let line_h = (bm.descent - bm.ascent).ceil() + LINE_GAP;
        let (_, bm_body) = cache.font.metrics();
        let body_line_h = (bm_body.descent - bm_body.ascent).ceil() + LINE_GAP;

        let max_w = PANEL_W - 2.0 * PAD;
        let reason_lines = wrap_text(&self.reason, &cache.small_font, max_w);
        let revert_lines = wrap_text("Changes have been reverted.", &cache.small_font, max_w);

        TITLE_H
            + PAD
            + body_line_h  // "Plan cannot be solved"
            + PAD / 2.0
            + line_h * reason_lines.len() as f32
            + PAD / 2.0
            + line_h * revert_lines.len() as f32
            + PAD
            + DISMISS_BTN_H
            + PAD
    }
}

impl FloatingWindow for ErrorWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let panel_h = self.compute_panel_h(cache);
        let panel = Self::panel_rect(width, height, panel_h);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from_argb(40, 0, 0, 0));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(panel.left + 4.0, panel.top + 4.0, PANEL_W, panel_h),
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Red top accent strip
        paint.set_color(Color::from(INPUT_BORDER_ERROR));
        let accent = Rect::from_xywh(panel.left, panel.top, PANEL_W, TITLE_H);
        canvas.save();
        canvas.clip_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), None, None);
        canvas.draw_rect(accent, &paint);
        canvas.restore();

        // Error icon — simple "✕" circle
        let icon_cx = panel.left + PAD + 14.0;
        let icon_cy = panel.top + TITLE_H / 2.0;
        paint.set_color(Color::from_argb(60, 255, 255, 255));
        canvas.draw_circle((icon_cx, icon_cy), 14.0, &paint);
        paint.set_color(Color::WHITE);
        paint.set_stroke_width(2.0);
        paint.set_style(PaintStyle::Stroke);
        let d = 5.5;
        canvas.draw_line(
            (icon_cx - d, icon_cy - d),
            (icon_cx + d, icon_cy + d),
            &paint,
        );
        canvas.draw_line(
            (icon_cx + d, icon_cy - d),
            (icon_cx - d, icon_cy + d),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);

        // Title text
        let title = "Plan cannot be solved";
        let (_, tm) = cache.font.metrics();
        let title_y = panel.top + (TITLE_H - (tm.descent - tm.ascent)) / 2.0 - tm.ascent;
        paint.set_color(Color::WHITE);
        if let Some(blob) = TextBlob::new(title, &cache.font) {
            canvas.draw_text_blob(&blob, (panel.left + PAD + 36.0, title_y), &paint);
        }

        // Body area
        let (_, bm) = cache.small_font.metrics();
        let line_h = (bm.descent - bm.ascent).ceil() + LINE_GAP;
        let (_, bm_body) = cache.font.metrics();
        let max_w = PANEL_W - 2.0 * PAD;

        let mut y = panel.top + TITLE_H + PAD;

        // Reason section
        let reason_lines = wrap_text(&self.reason, &cache.small_font, max_w);
        paint.set_color(Color::from(ITEM_FG));
        for line in &reason_lines {
            let draw_y = y - bm.ascent;
            if let Some(blob) = TextBlob::new(line.as_str(), &cache.small_font) {
                canvas.draw_text_blob(&blob, (panel.left + PAD, draw_y), &paint);
            }
            y += line_h;
        }

        y += PAD / 2.0;

        // "Changes have been reverted." in muted colour
        let revert_lines = wrap_text("Changes have been reverted.", &cache.small_font, max_w);
        paint.set_color(Color::from(MUTED_FG));
        for line in &revert_lines {
            let draw_y = y - bm.ascent;
            if let Some(blob) = TextBlob::new(line.as_str(), &cache.small_font) {
                canvas.draw_text_blob(&blob, (panel.left + PAD, draw_y), &paint);
            }
            y += line_h;
        }

        // Dismiss button
        let btn_y = panel.bottom - PAD - DISMISS_BTN_H;
        let btn_x = panel.right - PAD - DISMISS_BTN_W;
        let btn_rect = Rect::from_xywh(btn_x, btn_y, DISMISS_BTN_W, DISMISS_BTN_H);
        let btn_color = if self.hovered_dismiss {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        };
        paint.set_color(Color::from(btn_color));
        canvas.draw_rrect(
            RRect::new_rect_xy(btn_rect, CORNER / 2.0, CORNER / 2.0),
            &paint,
        );
        paint.set_color(Color::from(BTN_PRIMARY_FG));
        let lbl = "Dismiss";
        let (lbl_w, _) = cache.font.measure_str(lbl, None);
        let lbl_x = btn_x + (DISMISS_BTN_W - lbl_w) / 2.0;
        let lbl_y =
            btn_y + (DISMISS_BTN_H - (bm_body.descent - bm_body.ascent)) / 2.0 - bm_body.ascent;
        if let Some(blob) = TextBlob::new(lbl, &cache.font) {
            canvas.draw_text_blob(&blob, (lbl_x, lbl_y), &paint);
        }

        // Red stroke border
        paint.set_color(Color::from(INPUT_BORDER_ERROR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        _width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> FloatingWindowOutcome {
        let was = self.hovered_dismiss;
        self.hovered_dismiss = self.dismiss_rect.contains(Point::new(x, y));
        if self.hovered_dismiss != was {
            FloatingWindowOutcome::dirty(DirtyRegion::All)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        _modifiers: &winit::event::Modifiers,
        _plan: &Plan,
        _sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let panel_h = self.compute_panel_h(cache);
        let panel = Self::panel_rect(width, height, panel_h);
        let btn_y = panel.bottom - PAD - DISMISS_BTN_H;
        let btn_x = panel.right - PAD - DISMISS_BTN_W;
        self.dismiss_rect = Rect::from_xywh(btn_x, btn_y, DISMISS_BTN_W, DISMISS_BTN_H);

        if self.dismiss_rect.contains(skia_safe::Point::new(x, y)) {
            FloatingWindowOutcome::close()
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_dismiss = false;
    }
}
