//! Floating window for editing top-level plan settings.

use skia_safe::{Canvas, ClipOp, Color, Contains, Paint, PaintStyle, Point, RRect, Rect, TextBlob};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::data::Plan;
use crate::engine::{PlanRequest, PlanRequestSender};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, DIVIDER_COLOR,
    INPUT_BG, INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG,
    LABEL_FG, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING,
    PLAN_INPUT_H, PLAN_LABEL_GAP, TOOLBAR_STROKE_WIDTH,
};
use crate::ui::text_input::TextInput;

const PANEL_W: f32 = 500.0;
const PANEL_H: f32 = 320.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const SAVE_BTN_W: f32 = 80.0;
const CANCEL_BTN_W: f32 = 80.0;
const LABEL_H: f32 = 14.0;

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    StartDate,
}

pub struct PlanSettingsWindow {
    pub name: TextInput,
    pub start_date: TextInput,
    focused: Field,
    hovered_back: bool,
    hovered_save: bool,
    hovered_cancel: bool,
    error: Option<String>,
}

impl PlanSettingsWindow {
    pub fn new() -> Self {
        Self {
            name: TextInput::new(""),
            start_date: TextInput::new(""),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
            hovered_cancel: false,
            error: None,
        }
    }

    pub fn with_values(name: &str, date: &str) -> Self {
        let mut w = Self::new();
        w.name = TextInput::new(name);
        w.start_date = TextInput::new(date);
        w
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph)
    }

    fn back_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.left + BTN_INSET,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn save_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.right - PLAN_FORM_PADDING - SAVE_BTN_W,
            panel.bottom - PLAN_FORM_PADDING - PLAN_BTN_H,
            SAVE_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn cancel_btn_rect(width: f32, height: f32) -> Rect {
        let save = Self::save_btn_rect(width, height);
        Rect::from_xywh(
            save.left - 8.0 - CANCEL_BTN_W,
            save.top,
            CANCEL_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn name_input_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        let y = panel.top + TITLE_H + 1.0 + PLAN_FORM_PADDING + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(
            panel.left + PLAN_FORM_PADDING,
            y,
            panel.width() - 2.0 * PLAN_FORM_PADDING,
            PLAN_INPUT_H,
        )
    }

    fn start_date_input_rect(width: f32, height: f32) -> Rect {
        let name_rect = Self::name_input_rect(width, height);
        let y = name_rect.bottom + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(name_rect.left, y, name_rect.width(), PLAN_INPUT_H)
    }

    fn try_save(&self, sender: &PlanRequestSender) -> Result<(), String> {
        let name = self.name.content.trim().to_string();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        let date_str = self.start_date.content.trim().to_string();
        let date = date_str
            .parse::<chrono::NaiveDate>()
            .map_err(|_| "Start date must be YYYY-MM-DD".to_string())?;
        sender.send(PlanRequest::UpdatePlanSettings {
            name,
            start_date: date,
        });
        Ok(())
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused {
            Field::Name => &mut self.name,
            Field::StartDate => &mut self.start_date,
        }
    }
}

fn draw_text_input(
    canvas: &Canvas,
    rect: Rect,
    input: &TextInput,
    focused: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if focused {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let h_pad = 8.0;
    let inner = Rect::from_xywh(
        rect.left + h_pad,
        rect.top + 2.0,
        rect.width() - 2.0 * h_pad,
        rect.height() - 4.0,
    );

    let cursor_pos = input.cursor.min(input.content.len());
    let cursor_x_px = if cursor_pos == 0 {
        0.0f32
    } else {
        cache.font.measure_str(&input.content[..cursor_pos], None).0
    };

    let scroll_x = if focused {
        let inner_w = inner.width();
        let prev = input.scroll_x.get();
        let next = if cursor_x_px < prev {
            cursor_x_px
        } else if cursor_x_px > prev + inner_w {
            cursor_x_px - inner_w + 8.0
        } else {
            prev
        };
        input.scroll_x.set(next);
        next
    } else {
        0.0
    };

    canvas.save();
    canvas.clip_rect(inner, ClipOp::Intersect, false);

    let (_, metrics) = cache.font.metrics();
    let text_y =
        rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

    if !input.content.is_empty()
        && let Some(blob) = TextBlob::new(&input.content, &cache.font)
    {
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (inner.left - scroll_x, text_y), &paint);
    }
    if focused {
        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(
                inner.left + cursor_x_px - scroll_x,
                rect.top + 5.0,
                1.5,
                rect.height() - 10.0,
            ),
            &paint,
        );
    }
    canvas.restore();
}

impl FloatingWindow for PlanSettingsWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Shadow
        paint.set_color(Color::from(0x28_000000_u32));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(
                    panel.left + 2.0,
                    panel.top + 4.0,
                    panel.width(),
                    panel.height(),
                ),
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Title bar divider
        paint.set_color(Color::from(DIVIDER_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line(
            (panel.left, panel.top + TITLE_H),
            (panel.right, panel.top + TITLE_H),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);

        // Title text
        paint.set_color(Color::from(0xff_222222_u32));
        if let Some(blob) = TextBlob::new("Plan Settings", &cache.font) {
            let tw = cache.font.measure_str("Plan Settings", None).0;
            let (_, metrics) = cache.font.metrics();
            let tx = panel.left + (panel.width() - tw) / 2.0;
            let ty = panel.top + TITLE_H / 2.0 + (metrics.descent - metrics.ascent) / 2.0;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Back / close button
        let back_rect = Self::back_btn_rect(width, height);
        if self.hovered_back {
            paint.set_color(Color::from(BACK_BTN_HOVER_BG));
            canvas.draw_rrect(
                RRect::new_rect_xy(back_rect, BACK_BTN_CORNER, BACK_BTN_CORNER),
                &paint,
            );
        }
        paint.set_color(Color::from(BACK_BTN_ICON_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
        let m = back_rect.left + back_rect.width() * 0.28;
        let n = back_rect.left + back_rect.width() * 0.72;
        let mt = back_rect.top + back_rect.height() * 0.28;
        let nt = back_rect.top + back_rect.height() * 0.72;
        canvas.draw_line((m, mt), (n, nt), &paint);
        canvas.draw_line((n, mt), (m, nt), &paint);
        paint.set_style(PaintStyle::Fill);

        // Name field label
        let name_rect = Self::name_input_rect(width, height);
        let name_label_y = name_rect.top - PLAN_LABEL_GAP;
        paint.set_color(Color::from(LABEL_FG));
        if let Some(blob) = TextBlob::new("Plan Name", &cache.small_font) {
            let (_, metrics) = cache.small_font.metrics();
            canvas.draw_text_blob(
                &blob,
                (name_rect.left, name_label_y - metrics.ascent),
                &paint,
            );
        }
        draw_text_input(
            canvas,
            name_rect,
            &self.name,
            self.focused == Field::Name,
            cache,
        );

        // Start date field label
        let date_rect = Self::start_date_input_rect(width, height);
        let date_label_y = date_rect.top - PLAN_LABEL_GAP;
        paint.set_color(Color::from(LABEL_FG));
        if let Some(blob) = TextBlob::new("Start Date (YYYY-MM-DD)", &cache.small_font) {
            let (_, metrics) = cache.small_font.metrics();
            canvas.draw_text_blob(
                &blob,
                (date_rect.left, date_label_y - metrics.ascent),
                &paint,
            );
        }
        draw_text_input(
            canvas,
            date_rect,
            &self.start_date,
            self.focused == Field::StartDate,
            cache,
        );

        // Error message
        if let Some(err) = &self.error {
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            if let Some(blob) = TextBlob::new(err.as_str(), &cache.small_font) {
                let (_, metrics) = cache.small_font.metrics();
                canvas.draw_text_blob(
                    &blob,
                    (name_rect.left, date_rect.bottom + 8.0 - metrics.ascent),
                    &paint,
                );
            }
        }

        // Save button
        let save_rect = Self::save_btn_rect(width, height);
        let save_bg = if self.hovered_save {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        };
        paint.set_color(Color::from(save_bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(save_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(BTN_PRIMARY_FG));
        if let Some(blob) = TextBlob::new("Save", &cache.font) {
            let tw = cache.font.measure_str("Save", None).0;
            let (_, metrics) = cache.font.metrics();
            let tx = save_rect.left + (save_rect.width() - tw) / 2.0;
            let ty = save_rect.top
                + (save_rect.height() - (metrics.descent - metrics.ascent)) / 2.0
                - metrics.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Cancel button
        let cancel_rect = Self::cancel_btn_rect(width, height);
        let cancel_bg = if self.hovered_cancel {
            0xff_e0e0e0_u32
        } else {
            BTN_SECONDARY_BG
        };
        paint.set_color(Color::from(cancel_bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(cancel_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        if let Some(blob) = TextBlob::new("Cancel", &cache.font) {
            let tw = cache.font.measure_str("Cancel", None).0;
            let (_, metrics) = cache.font.metrics();
            let tx = cancel_rect.left + (cancel_rect.width() - tw) / 2.0;
            let ty = cancel_rect.top
                + (cancel_rect.height() - (metrics.descent - metrics.ascent)) / 2.0
                - metrics.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _plan: &Plan,
    ) -> FloatingWindowOutcome {
        let p = Point::new(x, y);
        let hb = Self::back_btn_rect(width, height).contains(p);
        let hs = Self::save_btn_rect(width, height).contains(p);
        let hc = Self::cancel_btn_rect(width, height).contains(p);
        if hb != self.hovered_back || hs != self.hovered_save || hc != self.hovered_cancel {
            self.hovered_back = hb;
            self.hovered_save = hs;
            self.hovered_cancel = hc;
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
        _modifiers: &Modifiers,
        _plan: &Plan,
        sender: &PlanRequestSender,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let p = Point::new(x, y);
        if Self::back_btn_rect(width, height).contains(p)
            || Self::cancel_btn_rect(width, height).contains(p)
        {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(p) {
            return match self.try_save(sender) {
                Ok(()) => FloatingWindowOutcome::close(),
                Err(e) => {
                    self.error = Some(e);
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                }
            };
        }
        if Self::name_input_rect(width, height).contains(p) {
            self.focused = Field::Name;
            self.name.focused = true;
            self.start_date.focused = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        if Self::start_date_input_rect(width, height).contains(p) {
            self.focused = Field::StartDate;
            self.name.focused = false;
            self.start_date.focused = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Tab) => {
                self.focused = match self.focused {
                    Field::Name => {
                        self.name.focused = false;
                        self.start_date.focused = true;
                        Field::StartDate
                    }
                    Field::StartDate => {
                        self.start_date.focused = false;
                        self.name.focused = true;
                        Field::Name
                    }
                };
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::Enter) => match self.try_save(sender) {
                Ok(()) => FloatingWindowOutcome::close(),
                Err(e) => {
                    self.error = Some(e);
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                }
            },
            Key::Named(NamedKey::Backspace) => {
                self.focused_input().backspace();
                self.error = None;
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.focused_input().move_left();
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.focused_input().move_right();
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::Home) => {
                self.focused_input().move_home();
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::End) => {
                self.focused_input().move_end();
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Named(NamedKey::Space) => {
                self.focused_input().insert_str(" ");
                self.error = None;
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    self.focused_input().insert_str(c.as_str());
                    self.error = None;
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
        self.hovered_cancel = false;
    }
}
