//! Floating window for editing a [`WorkSchedule`] — either the plan default or a
//! per-user override.

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::data::ids::UserId;
use crate::data::schedule::Weekday;
use crate::data::{Plan, WorkSchedule};
use crate::engine::{PlanRequest, PlanRequestSender};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, BTN_DANGER_BG, BTN_DANGER_FG, BTN_PRIMARY_BG, BTN_PRIMARY_FG,
    BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, DIVIDER_COLOR, ERROR_BG, INPUT_BG,
    INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG,
    LABEL_FG, LIST_BG, MUTED_FG, OVERLAY_SOFT, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H,
    PLAN_FIELD_GAP, PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP, SCROLLBAR_THUMB_COLOR,
};
use crate::ui::text_input::TextInput;

// ── Layout constants ──────────────────────────────────────────────────────────

const PANEL_W: f32 = 400.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const SCROLLBAR_W: f32 = 4.0;

const DAY_ROW_H: f32 = 32.0;
const DAY_ROW_GAP: f32 = 4.0;
const DAY_LABEL_W: f32 = 100.0;
const DAY_INPUT_W: f32 = 60.0;
const PRESETS_BTN_W: f32 = 112.0;
const CANCEL_BTN_W: f32 = 80.0;
const SAVE_BTN_W: f32 = 80.0;
const RESET_BTN_W: f32 = 160.0;

const SUBTITLE_H: f32 = 18.0;
const SUBTITLE_GAP: f32 = 4.0;

/// Computed full content height (preset row + gap + 7 day rows + gap + footer)
const PANEL_H: f32 = TITLE_H
    + 1.0
    + SUBTITLE_H
    + SUBTITLE_GAP
    + PLAN_FORM_PADDING
    + PLAN_BTN_H                              // presets row
    + PLAN_FIELD_GAP
    + (DAY_ROW_H + DAY_ROW_GAP) * 7.0        // 7 day rows
    + PLAN_FIELD_GAP
    + PLAN_BTN_H                              // footer buttons
    + PLAN_FORM_PADDING;

// ── Target enum ───────────────────────────────────────────────────────────────

/// Which schedule this window is editing.
#[derive(Clone)]
pub enum ScheduleTarget {
    PlanDefault,
    User(UserId),
}

// ── Helper types ──────────────────────────────────────────────────────────────

struct DayRow {
    day: Weekday,
    input: TextInput,
}

const ALL_DAYS: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

fn day_label(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn build_days(schedule: &WorkSchedule) -> Vec<DayRow> {
    ALL_DAYS
        .iter()
        .map(|&day| {
            let hours = schedule.hours_on(day);
            let text = if hours > 0.0 {
                // Format without trailing zeros: 8.0 → "8", 7.5 → "7.5"
                if hours == hours.floor() {
                    format!("{}", hours as u32)
                } else {
                    format!("{hours}")
                }
            } else {
                String::new()
            };
            DayRow {
                day,
                input: TextInput::new(&text),
            }
        })
        .collect()
}

// ── Main struct ───────────────────────────────────────────────────────────────

/// Floating window for editing a work schedule.
pub struct ScheduleWindow {
    target: ScheduleTarget,
    user_display_name: String,
    days: Vec<DayRow>,
    focused_day: Option<usize>,
    hovered_back: bool,
    hovered_save: bool,
    hovered_cancel: bool,
    hovered_weekdays: bool,
    hovered_fullweek: bool,
    hovered_reset: bool,
    scroll_y: f32,
    scheduler_error: Option<String>,
}

impl ScheduleWindow {
    /// Open the window to edit the plan's default schedule.
    pub fn for_plan(schedule: &WorkSchedule) -> Self {
        Self {
            target: ScheduleTarget::PlanDefault,
            user_display_name: "Plan Default".to_string(),
            days: build_days(schedule),
            focused_day: None,
            hovered_back: false,
            hovered_save: false,
            hovered_cancel: false,
            hovered_weekdays: false,
            hovered_fullweek: false,
            hovered_reset: false,
            scroll_y: 0.0,
            scheduler_error: None,
        }
    }

    /// Open the window to edit a specific user's schedule override.
    pub fn for_user(user_id: UserId, name: &str, schedule: &WorkSchedule) -> Self {
        Self {
            target: ScheduleTarget::User(user_id),
            user_display_name: name.to_string(),
            days: build_days(schedule),
            focused_day: None,
            hovered_back: false,
            hovered_save: false,
            hovered_cancel: false,
            hovered_weekdays: false,
            hovered_fullweek: false,
            hovered_reset: false,
            scroll_y: 0.0,
            scheduler_error: None,
        }
    }

    // ── Layout helpers ────────────────────────────────────────────────────────

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

    fn content_x(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).left + PLAN_FORM_PADDING
    }

    /// Y of the content top (below title + subtitle, before scroll), in content space.
    fn content_start_y(width: f32, height: f32) -> f32 {
        let panel = Self::panel_rect(width, height);
        panel.top + TITLE_H + 1.0 + SUBTITLE_H + SUBTITLE_GAP + PLAN_FORM_PADDING
    }

    fn weekdays_btn_rect(width: f32, height: f32) -> Rect {
        let cx = Self::content_x(width, height);
        let y = Self::content_start_y(width, height);
        Rect::from_xywh(cx + 62.0, y, PRESETS_BTN_W, PLAN_BTN_H)
    }

    fn fullweek_btn_rect(width: f32, height: f32) -> Rect {
        let r = Self::weekdays_btn_rect(width, height);
        Rect::from_xywh(r.right + 8.0, r.top, PRESETS_BTN_W, PLAN_BTN_H)
    }

    fn day_input_rect(day_idx: usize, width: f32, height: f32) -> Rect {
        let cx = Self::content_x(width, height);
        let presets_bottom = Self::weekdays_btn_rect(width, height).bottom;
        let rows_top = presets_bottom + PLAN_FIELD_GAP;
        let row_y = rows_top + day_idx as f32 * (DAY_ROW_H + DAY_ROW_GAP);
        let input_x = cx + DAY_LABEL_W + 8.0;
        let input_y = row_y + (DAY_ROW_H - PLAN_INPUT_H) / 2.0;
        Rect::from_xywh(input_x, input_y, DAY_INPUT_W, PLAN_INPUT_H)
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

    fn reset_btn_rect(width: f32, height: f32) -> Rect {
        let save = Self::save_btn_rect(width, height);
        let cx = Self::content_x(width, height);
        Rect::from_xywh(cx, save.top, RESET_BTN_W, PLAN_BTN_H)
    }

    fn effective_scroll(&self, width: f32, height: f32) -> f32 {
        let max = self.max_scroll(width, height);
        self.scroll_y.clamp(0.0, max)
    }

    fn max_scroll(&self, width: f32, height: f32) -> f32 {
        let panel = Self::panel_rect(width, height);
        let save_top = Self::save_btn_rect(width, height).top;
        let content_h = save_top - PLAN_FORM_PADDING - Self::content_start_y(width, height);
        let visible_h = panel.height()
            - TITLE_H
            - 1.0
            - SUBTITLE_H
            - SUBTITLE_GAP
            - PLAN_FORM_PADDING
            - PLAN_BTN_H
            - PLAN_FORM_PADDING;
        (content_h - visible_h).max(0.0)
    }

    fn to_content_y(&self, y: f32) -> f32 {
        y + self.scroll_y
    }

    // ── Actions ───────────────────────────────────────────────────────────────

    fn build_schedule(&self) -> WorkSchedule {
        let mut sched = WorkSchedule {
            days: std::collections::HashMap::new(),
        };
        for row in &self.days {
            let text = row.input.content.trim().to_string();
            if let Ok(h) = text.parse::<f32>()
                && h > 0.0
            {
                sched.days.insert(row.day, h);
            }
        }
        sched
    }

    fn apply_weekdays_preset(&mut self) {
        for row in &mut self.days {
            row.input = TextInput::new(match row.day {
                Weekday::Saturday | Weekday::Sunday => "",
                _ => "8",
            });
        }
    }

    fn apply_fullweek_preset(&mut self) {
        for row in &mut self.days {
            row.input = TextInput::new("8");
        }
    }

    fn try_submit(&mut self, plan: &Plan, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let schedule = self.build_schedule();
        // Dry-run: clone plan, apply change, run scheduler
        let mut dry = plan.clone();
        match &self.target {
            ScheduleTarget::PlanDefault => {
                dry.default_schedule = schedule.clone();
            }
            ScheduleTarget::User(uid) => {
                dry.set_user_schedule(*uid, schedule.clone());
            }
        }
        if let Err(e) = dry.compute_time_optimised_plan() {
            self.scheduler_error = Some(e.to_string());
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        // Send the real request
        match &self.target {
            ScheduleTarget::PlanDefault => {
                sender.send(PlanRequest::SetDefaultSchedule(schedule));
            }
            ScheduleTarget::User(uid) => {
                sender.send(PlanRequest::SetUserSchedule(*uid, schedule));
            }
        }
        FloatingWindowOutcome::close()
    }

    fn try_reset(&mut self, plan: &Plan, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let uid = match &self.target {
            ScheduleTarget::User(uid) => *uid,
            ScheduleTarget::PlanDefault => return FloatingWindowOutcome::default(),
        };
        // Dry-run
        let mut dry = plan.clone();
        dry.clear_user_schedule(&uid);
        if let Err(e) = dry.compute_time_optimised_plan() {
            self.scheduler_error = Some(e.to_string());
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        sender.send(PlanRequest::ClearUserSchedule(uid));
        FloatingWindowOutcome::close()
    }
}

// ── Draw helpers (module-local) ───────────────────────────────────────────────

fn draw_text_input_local(
    canvas: &Canvas,
    rect: Rect,
    input: &TextInput,
    focused: bool,
    error: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if error {
        Color::from(INPUT_BORDER_ERROR)
    } else if focused {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if error { 2.0 } else { 1.0 });
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

fn draw_btn(
    canvas: &Canvas,
    rect: Rect,
    label: &str,
    hovered: bool,
    primary: bool,
    danger: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let bg = if danger {
        BTN_DANGER_BG
    } else if primary {
        if hovered {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        }
    } else if hovered {
        0xff_e0e0e0_u32
    } else {
        BTN_SECONDARY_BG
    };
    let fg = if danger {
        BTN_DANGER_FG
    } else if primary {
        BTN_PRIMARY_FG
    } else {
        BTN_SECONDARY_FG
    };
    paint.set_color(Color::from(bg));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new(label, &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str(label, None);
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        let tx = rect.left + (rect.width() - adv) / 2.0;
        let ty = rect.top + (rect.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(fg));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }
}

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for ScheduleWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let scroll_y = self.effective_scroll(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from(OVERLAY_SOFT));
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

        // Title bar
        let title_rect = Rect::from_xywh(panel.left, panel.top, panel.width(), TITLE_H);
        paint.set_color(Color::from(LIST_BG));
        canvas.draw_rrect(RRect::new_rect_xy(title_rect, CORNER, CORNER), &paint);
        canvas.draw_rect(
            Rect::from_xywh(
                panel.left,
                panel.top + CORNER,
                panel.width(),
                TITLE_H - CORNER,
            ),
            &paint,
        );

        if let Some(blob) = TextBlob::new("Edit Schedule", &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str("Edit Schedule", None);
            let tx = panel.left + (panel.width() - advance) / 2.0;
            let ty =
                panel.top + (TITLE_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        crate::ui::window_chrome::draw_chevron_btn(canvas, back_btn, self.hovered_back);

        // Divider below title
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // Subtitle (user name or "Plan Default") below title bar, outside scroll
        if let Some(blob) = TextBlob::new(&self.user_display_name, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(&self.user_display_name, None);
            let (_, sm) = cache.small_font.metrics();
            let sm_h = sm.descent - sm.ascent;
            let tx = panel.left + (panel.width() - adv) / 2.0;
            let ty = panel.top + TITLE_H + 1.0 + (SUBTITLE_H - sm_h) / 2.0 - sm.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Clip content area (below subtitle)
        let content_clip = Rect::from_xywh(
            panel.left,
            panel.top + TITLE_H + 1.0 + SUBTITLE_H + SUBTITLE_GAP,
            panel.width(),
            panel.height() - TITLE_H - 1.0 - SUBTITLE_H - SUBTITLE_GAP,
        );
        canvas.save();
        canvas.clip_rect(content_clip, ClipOp::Intersect, false);
        canvas.translate((0.0, -scroll_y));

        let cx = Self::content_x(width, height);
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;

        // ── Presets row ───────────────────────────────────────────────────────
        let presets_y = Self::content_start_y(width, height);
        if let Some(blob) = TextBlob::new("Presets", &cache.small_font) {
            let ty = presets_y + (PLAN_BTN_H - sm_h) / 2.0 - sm.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (cx, ty), &paint);
        }
        draw_btn(
            canvas,
            Self::weekdays_btn_rect(width, height),
            "Weekdays",
            self.hovered_weekdays,
            false,
            false,
            cache,
        );
        draw_btn(
            canvas,
            Self::fullweek_btn_rect(width, height),
            "Full Week",
            self.hovered_fullweek,
            false,
            false,
            cache,
        );

        // ── Day rows ──────────────────────────────────────────────────────────
        for (i, row) in self.days.iter().enumerate() {
            let input_rect = Self::day_input_rect(i, width, height);
            let row_y = input_rect.top - (DAY_ROW_H - PLAN_INPUT_H) / 2.0;
            let label = day_label(row.day);
            let is_weekend = matches!(row.day, Weekday::Saturday | Weekday::Sunday);
            let is_empty = row.input.content.trim().is_empty();
            let label_color = if is_weekend && is_empty {
                MUTED_FG
            } else {
                ITEM_FG
            };

            if let Some(blob) = TextBlob::new(label, &cache.small_font) {
                let (_, m) = cache.small_font.metrics();
                let mh = m.descent - m.ascent;
                let ty = row_y + (DAY_ROW_H - mh) / 2.0 - m.ascent;
                paint.set_color(Color::from(label_color));
                canvas.draw_text_blob(&blob, (cx, ty), &paint);
            }

            let focused = self.focused_day == Some(i);
            draw_text_input_local(canvas, input_rect, &row.input, focused, false, cache);

            // "hrs" label
            if let Some(blob) = TextBlob::new("hrs", &cache.small_font) {
                let (_, m) = cache.small_font.metrics();
                let mh = m.descent - m.ascent;
                let ty = input_rect.top + (PLAN_INPUT_H - mh) / 2.0 - m.ascent;
                paint.set_color(Color::from(MUTED_FG));
                canvas.draw_text_blob(&blob, (input_rect.right + 6.0, ty), &paint);
            }
        }

        canvas.restore();

        // ── Footer buttons (not clipped, pinned to panel bottom) ─────────────
        draw_btn(
            canvas,
            Self::save_btn_rect(width, height),
            "Save",
            self.hovered_save,
            true,
            false,
            cache,
        );
        draw_btn(
            canvas,
            Self::cancel_btn_rect(width, height),
            "Cancel",
            self.hovered_cancel,
            false,
            false,
            cache,
        );
        if matches!(self.target, ScheduleTarget::User(_)) {
            draw_btn(
                canvas,
                Self::reset_btn_rect(width, height),
                "Reset to Default",
                self.hovered_reset,
                false,
                true,
                cache,
            );
        }

        // ── Error banner ──────────────────────────────────────────────────────
        if let Some(ref err_msg) = self.scheduler_error {
            let banner_x = panel.left + PLAN_FORM_PADDING;
            let banner_w = panel.width() - 2.0 * PLAN_FORM_PADDING;
            let (_, sm_m) = cache.small_font.metrics();
            let line_h = sm_m.descent - sm_m.ascent + 4.0;
            // Simple word-wrap
            let words: Vec<&str> = err_msg.split_whitespace().collect();
            let mut lines: Vec<String> = Vec::new();
            let mut cur = String::new();
            for word in &words {
                let probe = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if cache.small_font.measure_str(&probe, None).0 > banner_w - 16.0 && !cur.is_empty()
                {
                    lines.push(cur.clone());
                    cur = word.to_string();
                } else {
                    cur = probe;
                }
            }
            if !cur.is_empty() {
                lines.push(cur);
            }
            if lines.is_empty() {
                lines.push(err_msg.clone());
            }
            let banner_h = 8.0 + lines.len() as f32 * line_h + 8.0;
            let banner_y = Self::save_btn_rect(width, height).top - 8.0 - banner_h;
            let banner_rect = Rect::from_xywh(banner_x, banner_y, banner_w, banner_h);
            paint.set_color(Color::from(ERROR_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(banner_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.0);
            canvas.draw_rrect(
                RRect::new_rect_xy(banner_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            paint.set_style(PaintStyle::Fill);
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            for (i, line) in lines.iter().enumerate() {
                if let Some(blob) = TextBlob::new(line, &cache.small_font) {
                    let ty = banner_y + 8.0 + i as f32 * line_h - sm_m.ascent;
                    canvas.draw_text_blob(&blob, (banner_x + 8.0, ty), &paint);
                }
            }
        }

        // ── Scrollbar ─────────────────────────────────────────────────────────
        let max = self.max_scroll(width, height);
        if max > 0.0 {
            let content_clip_h = panel.height() - TITLE_H - 1.0 - SUBTITLE_H - SUBTITLE_GAP;
            let content_h = content_clip_h + max;
            let thumb_h = (content_clip_h * content_clip_h / content_h).max(20.0);
            let track_top = panel.top + TITLE_H + 1.0 + SUBTITLE_H + SUBTITLE_GAP;
            let scroll_y_eff = self.effective_scroll(width, height);
            let thumb_y = track_top + (scroll_y_eff / max) * (content_clip_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        panel.right - SCROLLBAR_W - 2.0,
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
        let cy = self.to_content_y(y);
        let cp = Point::new(x, cy);

        let new_back = Self::back_btn_rect(width, height).contains(p);
        let new_save = Self::save_btn_rect(width, height).contains(p);
        let new_cancel = Self::cancel_btn_rect(width, height).contains(p);
        let new_weekdays = Self::weekdays_btn_rect(width, height).contains(cp);
        let new_fullweek = Self::fullweek_btn_rect(width, height).contains(cp);
        let new_reset = matches!(self.target, ScheduleTarget::User(_))
            && Self::reset_btn_rect(width, height).contains(p);

        if new_back != self.hovered_back
            || new_save != self.hovered_save
            || new_cancel != self.hovered_cancel
            || new_weekdays != self.hovered_weekdays
            || new_fullweek != self.hovered_fullweek
            || new_reset != self.hovered_reset
        {
            self.hovered_back = new_back;
            self.hovered_save = new_save;
            self.hovered_cancel = new_cancel;
            self.hovered_weekdays = new_weekdays;
            self.hovered_fullweek = new_fullweek;
            self.hovered_reset = new_reset;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        _modifiers: &Modifiers,
        plan: &Plan,
        sender: &PlanRequestSender,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let p = Point::new(x, y);
        let cy = self.to_content_y(y);
        let cp = Point::new(x, cy);

        // Back / Cancel close
        if Self::back_btn_rect(width, height).contains(p)
            || Self::cancel_btn_rect(width, height).contains(p)
        {
            return FloatingWindowOutcome::close();
        }

        // Save
        if Self::save_btn_rect(width, height).contains(p) {
            return self.try_submit(plan, sender);
        }

        // Reset (user only)
        if matches!(self.target, ScheduleTarget::User(_))
            && Self::reset_btn_rect(width, height).contains(p)
        {
            return self.try_reset(plan, sender);
        }

        // Weekdays preset
        if Self::weekdays_btn_rect(width, height).contains(cp) {
            self.apply_weekdays_preset();
            self.scheduler_error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Full Week preset
        if Self::fullweek_btn_rect(width, height).contains(cp) {
            self.apply_fullweek_preset();
            self.scheduler_error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Day input clicks
        for i in 0..self.days.len() {
            if Self::day_input_rect(i, width, height).contains(cp) {
                if self.focused_day != Some(i) {
                    self.focused_day = Some(i);
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
                return FloatingWindowOutcome::default();
            }
        }

        // Click outside panel
        if !Self::panel_rect(width, height).contains(p) {
            return FloatingWindowOutcome::close();
        }

        // Click inside panel but not on anything — defocus
        if self.focused_day.is_some() {
            self.focused_day = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        sender: &PlanRequestSender,
        width: f32,
        height: f32,
        plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if *key == Key::Named(NamedKey::Escape) {
            return FloatingWindowOutcome::close();
        }
        if *key == Key::Named(NamedKey::Enter) {
            return self.try_submit(plan, sender);
        }
        if *key == Key::Named(NamedKey::Tab) {
            let next = match self.focused_day {
                Some(i) => (i + 1) % self.days.len(),
                None => 0,
            };
            self.focused_day = Some(next);
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        if let Some(idx) = self.focused_day {
            let row = &mut self.days[idx];
            match key {
                Key::Named(NamedKey::Backspace) => {
                    row.input.backspace();
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    row.input.move_left();
                }
                Key::Named(NamedKey::ArrowRight) => {
                    row.input.move_right();
                }
                Key::Named(NamedKey::Home) => {
                    row.input.move_home();
                }
                Key::Named(NamedKey::End) => {
                    row.input.move_end();
                }
                Key::Character(s) => {
                    // Only allow digits and one decimal point
                    for ch in s.chars() {
                        if ch.is_ascii_digit() || (ch == '.' && !row.input.content.contains('.')) {
                            row.input.insert_str(&ch.to_string());
                        }
                    }
                }
                _ => return FloatingWindowOutcome::default(),
            }
            self.scheduler_error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // No focused day — use default handler (Escape already handled above)
        let _ = (width, height);
        FloatingWindowOutcome::default()
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        let max = self.max_scroll(width, height);
        if max <= 0.0 {
            return FloatingWindowOutcome::default();
        }
        let new_offset = (self.scroll_y - delta_y * 40.0).clamp(0.0, max);
        if (new_offset - self.scroll_y).abs() > f32::EPSILON {
            self.scroll_y = new_offset;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
        self.hovered_cancel = false;
        self.hovered_weekdays = false;
        self.hovered_fullweek = false;
        self.hovered_reset = false;
    }
}
