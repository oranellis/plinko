//! Floating window for creating a new plan.
//!
//! Collects a plan name and start date, then sends [`PlanRequest::NewPlan`]
//! followed by [`PlanRequest::UpdatePlanSettings`] with the provided values.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome, panel_size};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, CAL_SELECTED_BG,
    DIVIDER_COLOR, INPUT_BG, INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS,
    INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG, MUTED_FG, OVERLAY_LIGHT, OVERLAY_SOFT,
    OVERLAY_XLIGHT, PANEL_BG, PANEL_TEXT, PLACEHOLDER_FG, PLAN_BTN_CORNER, PLAN_BTN_H,
    PLAN_FIELD_GAP, PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP, SUBTLE_FG,
    TOOLBAR_BTN_HOVER_BG, TOOLBAR_STROKE_WIDTH,
};
use crate::ui::text_input::TextInput;
use plinko_shared::data::Plan;
use plinko_shared::data::ids::NodeId;
use plinko_shared::protocol::PlanRequest;

// ── Layout ────────────────────────────────────────────────────────────────────

const PANEL_W: f32 = 400.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const LABEL_H: f32 = 14.0;
const FIELD_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H;
const PANEL_H: f32 = TITLE_H
    + 1.0
    + PLAN_FORM_PADDING
    + FIELD_BLOCK_H   // name
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // start date
    + LABEL_H         // error row
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;
const CREATE_BTN_W: f32 = 90.0;
const CANCEL_BTN_W: f32 = 80.0;

// Calendar constants
const CAL_PAD: f32 = 8.0;
const CAL_CELL: f32 = 32.0;
const CAL_W: f32 = CAL_CELL * 7.0 + CAL_PAD * 2.0;
const CAL_HEADER_H: f32 = 28.0;
const CAL_DOW_H: f32 = 20.0;
const CAL_ROW_H: f32 = 26.0;
const CAL_FOOTER_H: f32 = 28.0;
const CAL_H: f32 = CAL_PAD + CAL_HEADER_H + CAL_DOW_H + CAL_ROW_H * 6.0 + CAL_FOOTER_H + CAL_PAD;

// ── CalendarPicker ────────────────────────────────────────────────────────────

struct CalendarPicker {
    value: NaiveDate,
    nav_year: i32,
    nav_month: u32,
    hovered_day: Option<u32>,
    hovered_prev_year: bool,
    hovered_prev_month: bool,
    hovered_next_month: bool,
    hovered_next_year: bool,
    hovered_today: bool,
    hovered_trigger: bool,
}

impl CalendarPicker {
    fn new(value: NaiveDate) -> Self {
        Self {
            nav_year: value.year(),
            nav_month: value.month(),
            value,
            hovered_day: None,
            hovered_prev_year: false,
            hovered_prev_month: false,
            hovered_next_month: false,
            hovered_next_year: false,
            hovered_today: false,
            hovered_trigger: false,
        }
    }

    fn prev_month(&mut self) {
        if self.nav_month == 1 {
            self.nav_month = 12;
            self.nav_year -= 1;
        } else {
            self.nav_month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.nav_month == 12 {
            self.nav_month = 1;
            self.nav_year += 1;
        } else {
            self.nav_month += 1;
        }
    }

    fn prev_year(&mut self) {
        self.nav_year -= 1;
    }

    fn next_year(&mut self) {
        self.nav_year += 1;
    }

    fn reset_hover(&mut self) {
        self.hovered_day = None;
        self.hovered_prev_year = false;
        self.hovered_prev_month = false;
        self.hovered_next_month = false;
        self.hovered_next_year = false;
        self.hovered_today = false;
        self.hovered_trigger = false;
    }

    fn display_text(&self) -> String {
        format!("{}", self.value.format("%d %b %Y"))
    }
}

// ── Calendar geometry helpers ─────────────────────────────────────────────────

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (next.unwrap() - first).num_days() as u32
}

fn first_weekday_offset(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap()
        .weekday()
        .num_days_from_monday()
}

fn cal_prev_year_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.left + CAL_PAD,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_prev_month_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.left + CAL_PAD + CAL_HEADER_H + 2.0,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_next_month_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.right - CAL_PAD - 2.0 * CAL_HEADER_H - 2.0,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_next_year_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.right - CAL_PAD - CAL_HEADER_H,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_today_btn(cal: Rect) -> Rect {
    let w = 48.0;
    let h = 22.0;
    Rect::from_xywh(cal.left + CAL_PAD, cal.bottom - CAL_PAD - h, w, h)
}

fn cal_day_cell(cal: Rect, day_1_offset: u32, day: u32) -> Rect {
    let grid_idx = day_1_offset + day - 1;
    let col = grid_idx % 7;
    let row = grid_idx / 7;
    let grid_top = cal.top + CAL_PAD + CAL_HEADER_H + CAL_DOW_H;
    Rect::from_xywh(
        cal.left + CAL_PAD + col as f32 * CAL_CELL,
        grid_top + row as f32 * CAL_ROW_H,
        CAL_CELL,
        CAL_ROW_H,
    )
}

fn calendar_popup_rect(trigger_screen: Rect, panel: Rect) -> Rect {
    let below = trigger_screen.bottom + 4.0;
    let above = trigger_screen.top - 4.0 - CAL_H;
    let top = if below + CAL_H <= panel.bottom + 8.0 {
        below
    } else {
        above
    };
    let left = (trigger_screen.left + (trigger_screen.width() - CAL_W) / 2.0)
        .max(panel.left + 4.0)
        .min(panel.right - CAL_W - 4.0);
    Rect::from_xywh(left, top, CAL_W, CAL_H)
}

// ── NewPlanWindow ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Name,
}

pub struct NewPlanWindow {
    name: TextInput,
    start_date: CalendarPicker,
    cal_open: bool,
    focused: Field,
    hovered_back: bool,
    hovered_create: bool,
    hovered_cancel: bool,
    name_error: bool,
}

impl NewPlanWindow {
    pub fn new() -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            name: TextInput::new(""),
            start_date: CalendarPicker::new(today),
            cal_open: false,
            focused: Field::Name,
            hovered_back: false,
            hovered_create: false,
            hovered_cancel: false,
            name_error: false,
        }
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let (pw, ph) = panel_size(width, height, PANEL_W, PANEL_H);
        let x = (width - pw) / 2.0;
        let y = (height - ph) / 2.0;
        Rect::from_xywh(x, y, pw, ph)
    }

    fn name_field_rect(panel: Rect) -> Rect {
        let x = panel.left + PLAN_FORM_PADDING;
        let y = panel.top + TITLE_H + 1.0 + PLAN_FORM_PADDING + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, panel.width() - 2.0 * PLAN_FORM_PADDING, PLAN_INPUT_H)
    }

    fn date_trigger_rect(panel: Rect) -> Rect {
        let x = panel.left + PLAN_FORM_PADDING;
        let y = panel.top
            + TITLE_H
            + 1.0
            + PLAN_FORM_PADDING
            + FIELD_BLOCK_H
            + PLAN_FIELD_GAP
            + LABEL_H
            + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, panel.width() - 2.0 * PLAN_FORM_PADDING, PLAN_INPUT_H)
    }

    fn footer_btns(panel: Rect) -> (Rect, Rect) {
        let by = panel.bottom - PLAN_FORM_PADDING - PLAN_BTN_H;
        let create = Rect::from_xywh(
            panel.right - PLAN_FORM_PADDING - CREATE_BTN_W,
            by,
            CREATE_BTN_W,
            PLAN_BTN_H,
        );
        let cancel = Rect::from_xywh(
            create.left - 8.0 - CANCEL_BTN_W,
            by,
            CANCEL_BTN_W,
            PLAN_BTN_H,
        );
        (create, cancel)
    }

    fn back_btn_rect(panel: Rect) -> Rect {
        Rect::from_xywh(
            panel.left + BTN_INSET,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }
}

// ── Draw helpers ──────────────────────────────────────────────────────────────

fn draw_text_input(
    canvas: &Canvas,
    rect: Rect,
    input: &TextInput,
    focused: bool,
    error: bool,
    placeholder: &str,
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
    if input.content.is_empty() && !placeholder.is_empty() {
        if let Some(blob) = TextBlob::new(placeholder, &cache.font) {
            paint.set_color(Color::from(PLACEHOLDER_FG));
            canvas.draw_text_blob(&blob, (inner.left, text_y), &paint);
        }
    } else if let Some(blob) = TextBlob::new(&input.content, &cache.font) {
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

fn draw_date_trigger(
    canvas: &Canvas,
    rect: Rect,
    picker: &CalendarPicker,
    is_open: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if is_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else if picker.hovered_trigger {
        Color::from(MUTED_FG)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let text = picker.display_text();
    if let Some(blob) = TextBlob::new(&text, &cache.font) {
        let (_, m) = cache.font.metrics();
        let ty = rect.top + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Calendar icon
    let icon_cx = rect.right - 16.0;
    let icon_cy = rect.top + rect.height() / 2.0;
    let hs = 5.0;
    paint.set_color(Color::from(SUBTLE_FG));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.2);
    canvas.draw_rect(
        Rect::from_xywh(icon_cx - hs, icon_cy - hs * 0.8, hs * 2.0, hs * 1.8),
        &paint,
    );
    let mut pb = PathBuilder::new();
    pb.move_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8 - 3.0));
    pb.move_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8 - 3.0));
    canvas.draw_path(&pb.detach(), &paint);
}

fn draw_calendar_popup(canvas: &Canvas, cal: Rect, picker: &CalendarPicker, cache: &RenderCache) {
    let today = chrono::Local::now().date_naive();
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_LIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(cal.left + 2.0, cal.top + 4.0, cal.width(), cal.height()),
            CORNER,
            CORNER,
        ),
        &paint,
    );

    paint.set_color(Color::from(PANEL_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_style(PaintStyle::Fill);

    let (_, sm) = cache.small_font.metrics();
    let sm_h = sm.descent - sm.ascent;

    let nav_btns = [
        (cal_prev_year_btn(cal), picker.hovered_prev_year, -2i32),
        (cal_prev_month_btn(cal), picker.hovered_prev_month, -1),
        (cal_next_month_btn(cal), picker.hovered_next_month, 1),
        (cal_next_year_btn(cal), picker.hovered_next_year, 2),
    ];
    for (btn, hov, dir) in nav_btns {
        let bg = if hov { TOOLBAR_BTN_HOVER_BG } else { INPUT_BG };
        paint.set_color(Color::from(bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        let cx = btn.left + btn.width() / 2.0;
        let cy = btn.top + btn.height() / 2.0;
        let double = dir.abs() == 2;
        let s = 4.0;
        let offset = if double { 2.5 } else { 0.0 };
        paint.set_color(Color::from(ITEM_FG));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        let double_shifts = [-offset, offset];
        let single_shift = [0.0f32];
        let shifts: &[f32] = if double {
            &double_shifts
        } else {
            &single_shift
        };
        for shift in shifts {
            let ox = if dir < 0 { *shift } else { -*shift };
            let mut pb = PathBuilder::new();
            if dir < 0 {
                pb.move_to((cx + ox + s * 0.45, cy - s));
                pb.line_to((cx + ox - s * 0.45, cy));
                pb.line_to((cx + ox + s * 0.45, cy + s));
            } else {
                pb.move_to((cx + ox - s * 0.45, cy - s));
                pb.line_to((cx + ox + s * 0.45, cy));
                pb.line_to((cx + ox - s * 0.45, cy + s));
            }
            canvas.draw_path(&pb.detach(), &paint);
        }
        paint.set_style(PaintStyle::Fill);
    }

    // Month/year header
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_str = format!(
        "{} {}",
        month_names[(picker.nav_month as usize).saturating_sub(1).min(11)],
        picker.nav_year
    );
    if let Some(blob) = TextBlob::new(&month_str, &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str(&month_str, None);
        let tx = cal.left + (cal.width() - adv) / 2.0;
        let ty = cal_prev_year_btn(cal).top + (CAL_HEADER_H - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(ITEM_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    // Day-of-week headers
    let dow_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let dow_y_top = cal.top + CAL_PAD + CAL_HEADER_H;
    for (i, lbl) in dow_labels.iter().enumerate() {
        let cx = cal.left + CAL_PAD + i as f32 * CAL_CELL + CAL_CELL / 2.0;
        if let Some(blob) = TextBlob::new(lbl, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(lbl, None);
            let ty = dow_y_top + (CAL_DOW_H - sm_h) / 2.0 - sm.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (cx - adv / 2.0, ty), &paint);
        }
    }

    // Day cells
    let day_1_offset = first_weekday_offset(picker.nav_year, picker.nav_month);
    let num_days = days_in_month(picker.nav_year, picker.nav_month);
    let today_in_nav = today.year() == picker.nav_year && today.month() == picker.nav_month;

    for day in 1..=num_days {
        let cell = cal_day_cell(cal, day_1_offset, day);
        let is_selected = picker.value.year() == picker.nav_year
            && picker.value.month() == picker.nav_month
            && picker.value.day() == day;
        let is_today = today_in_nav && today.day() == day;
        let is_hov = picker.hovered_day == Some(day);
        let cx = cell.left + cell.width() / 2.0;
        let cy = cell.top + cell.height() / 2.0;

        if is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        } else if is_hov {
            paint.set_color(Color::from(CAL_SELECTED_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        }

        let day_str = format!("{day}");
        if let Some(blob) = TextBlob::new(&day_str, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(&day_str, None);
            let tx = cx - adv / 2.0;
            let ty = cell.top + (cell.height() - sm_h) / 2.0 - sm.ascent;
            paint.set_color(if is_selected {
                Color::WHITE
            } else if is_today {
                Color::from(BTN_PRIMARY_BG)
            } else {
                Color::from(ITEM_FG)
            });
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
        if is_today && !is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cell.top + cell.height() - 4.0), 2.0, &paint);
        }
    }

    // Footer
    let footer_y = cal.bottom - CAL_PAD - CAL_FOOTER_H;
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(cal.left, footer_y, cal.width(), 1.0),
        &paint,
    );

    let today_btn = cal_today_btn(cal);
    paint.set_color(Color::from(if picker.hovered_today {
        TOOLBAR_BTN_HOVER_BG
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(today_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Today", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Today", None);
        let tx = today_btn.left + (today_btn.width() - adv) / 2.0;
        let ty = today_btn.top + (today_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }
}

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for NewPlanWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Panel shadow
        paint.set_color(Color::from(OVERLAY_SOFT));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(
                    panel.left + 3.0,
                    panel.top + 6.0,
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
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);
        paint.set_color(Color::from(OVERLAY_XLIGHT));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);
        paint.set_style(PaintStyle::Fill);

        // Title divider
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // Title text
        if let Some(blob) = TextBlob::new("New Plan", &cache.font) {
            let (adv, _) = cache.font.measure_str("New Plan", None);
            let (_, m) = cache.font.metrics();
            let tx = panel.left + (panel.width() - adv) / 2.0;
            let ty = panel.top + (TITLE_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(PANEL_TEXT));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Back / close button
        let back_rect = Self::back_btn_rect(panel);
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
        let cx = back_rect.left + back_rect.width() / 2.0;
        let cy = back_rect.top + back_rect.height() / 2.0;
        let s = 5.0;
        canvas.draw_line((cx - s, cy - s), (cx + s, cy + s), &paint);
        canvas.draw_line((cx + s, cy - s), (cx - s, cy + s), &paint);
        paint.set_style(PaintStyle::Fill);

        // ── Name field ────────────────────────────────────────────────────
        let name_label_y = panel.top + TITLE_H + 1.0 + PLAN_FORM_PADDING;
        if let Some(blob) = TextBlob::new("Plan Name", &cache.small_font) {
            let (_, m) = cache.small_font.metrics();
            let ty = name_label_y + (LABEL_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (panel.left + PLAN_FORM_PADDING, ty), &paint);
        }
        draw_text_input(
            canvas,
            Self::name_field_rect(panel),
            &self.name,
            self.focused == Field::Name,
            self.name_error,
            "Enter plan name…",
            cache,
        );
        if self.name_error {
            let err_y = Self::name_field_rect(panel).bottom + 4.0;
            if let Some(blob) = TextBlob::new("Name is required", &cache.small_font) {
                let (_, m) = cache.small_font.metrics();
                let ty = err_y + (LABEL_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
                paint.set_color(Color::from(INPUT_BORDER_ERROR));
                canvas.draw_text_blob(&blob, (panel.left + PLAN_FORM_PADDING, ty), &paint);
            }
        }

        // ── Start date field ──────────────────────────────────────────────
        let date_label_y =
            panel.top + TITLE_H + 1.0 + PLAN_FORM_PADDING + FIELD_BLOCK_H + PLAN_FIELD_GAP;
        if let Some(blob) = TextBlob::new("Start Date", &cache.small_font) {
            let (_, m) = cache.small_font.metrics();
            let ty = date_label_y + (LABEL_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (panel.left + PLAN_FORM_PADDING, ty), &paint);
        }
        draw_date_trigger(
            canvas,
            Self::date_trigger_rect(panel),
            &self.start_date,
            self.cal_open,
            cache,
        );

        // ── Footer buttons ────────────────────────────────────────────────
        let (create_rect, cancel_rect) = Self::footer_btns(panel);
        let create_bg = if self.hovered_create {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        };
        paint.set_color(Color::from(create_bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(create_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        if let Some(blob) = TextBlob::new("Create", &cache.font) {
            let (adv, _) = cache.font.measure_str("Create", None);
            let (_, m) = cache.font.metrics();
            let tx = create_rect.left + (create_rect.width() - adv) / 2.0;
            let ty =
                create_rect.top + (create_rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(BTN_PRIMARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        paint.set_color(Color::from(if self.hovered_cancel {
            TOOLBAR_BTN_HOVER_BG
        } else {
            BTN_SECONDARY_BG
        }));
        canvas.draw_rrect(
            RRect::new_rect_xy(cancel_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        if let Some(blob) = TextBlob::new("Cancel", &cache.font) {
            let (adv, _) = cache.font.measure_str("Cancel", None);
            let (_, m) = cache.font.metrics();
            let tx = cancel_rect.left + (cancel_rect.width() - adv) / 2.0;
            let ty =
                cancel_rect.top + (cancel_rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(BTN_SECONDARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // ── Calendar popup (drawn last so it overlaps everything) ─────────
        if self.cal_open {
            let trigger = Self::date_trigger_rect(panel);
            let cal = calendar_popup_rect(trigger, panel);
            draw_calendar_popup(canvas, cal, &self.start_date, cache);
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
        let panel = Self::panel_rect(width, height);
        let pt = Point::new(x, y);
        let mut dirty = false;

        macro_rules! hover {
            ($field:expr, $rect:expr) => {{
                let v = $rect.contains(pt);
                if $field != v {
                    $field = v;
                    dirty = true;
                }
            }};
        }

        hover!(self.hovered_back, Self::back_btn_rect(panel));
        hover!(self.hovered_create, Self::footer_btns(panel).0);
        hover!(self.hovered_cancel, Self::footer_btns(panel).1);
        hover!(
            self.start_date.hovered_trigger,
            Self::date_trigger_rect(panel)
        );

        if self.cal_open {
            let trigger = Self::date_trigger_rect(panel);
            let cal = calendar_popup_rect(trigger, panel);
            let day_1_offset =
                first_weekday_offset(self.start_date.nav_year, self.start_date.nav_month);
            let num_days = days_in_month(self.start_date.nav_year, self.start_date.nav_month);
            let mut new_hov = None;
            for day in 1..=num_days {
                if cal_day_cell(cal, day_1_offset, day).contains(pt) {
                    new_hov = Some(day);
                    break;
                }
            }
            if self.start_date.hovered_day != new_hov {
                self.start_date.hovered_day = new_hov;
                dirty = true;
            }
            macro_rules! cal_hover {
                ($field:expr, $rect:expr) => {{
                    let v = $rect.contains(pt);
                    if $field != v {
                        $field = v;
                        dirty = true;
                    }
                }};
            }
            cal_hover!(self.start_date.hovered_prev_year, cal_prev_year_btn(cal));
            cal_hover!(self.start_date.hovered_prev_month, cal_prev_month_btn(cal));
            cal_hover!(self.start_date.hovered_next_month, cal_next_month_btn(cal));
            cal_hover!(self.start_date.hovered_next_year, cal_next_year_btn(cal));
            cal_hover!(self.start_date.hovered_today, cal_today_btn(cal));
        }

        if dirty {
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
        let panel = Self::panel_rect(width, height);
        let pt = Point::new(x, y);

        // Close button
        if Self::back_btn_rect(panel).contains(pt) {
            return FloatingWindowOutcome::close();
        }

        // Cancel
        if Self::footer_btns(panel).1.contains(pt) {
            return FloatingWindowOutcome::close();
        }

        // Create
        if Self::footer_btns(panel).0.contains(pt) {
            let name = self.name.content.trim().to_string();
            if name.is_empty() {
                self.name_error = true;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            let start_date = self.start_date.value;
            sender.send(PlanRequest::NewPlan);
            sender.send(PlanRequest::UpdatePlanSettings {
                name,
                start_date,
                scheduler_target: NodeId::PlanStart,
            });
            return FloatingWindowOutcome::close();
        }

        // Calendar popup interactions
        if self.cal_open {
            let trigger = Self::date_trigger_rect(panel);
            let cal = calendar_popup_rect(trigger, panel);

            if cal_prev_year_btn(cal).contains(pt) {
                self.start_date.prev_year();
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            if cal_prev_month_btn(cal).contains(pt) {
                self.start_date.prev_month();
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            if cal_next_month_btn(cal).contains(pt) {
                self.start_date.next_month();
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            if cal_next_year_btn(cal).contains(pt) {
                self.start_date.next_year();
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            if cal_today_btn(cal).contains(pt) {
                let today = chrono::Local::now().date_naive();
                self.start_date.value = today;
                self.start_date.nav_year = today.year();
                self.start_date.nav_month = today.month();
                self.cal_open = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }

            // Day cells
            let day_1_offset =
                first_weekday_offset(self.start_date.nav_year, self.start_date.nav_month);
            let num_days = days_in_month(self.start_date.nav_year, self.start_date.nav_month);
            for day in 1..=num_days {
                if cal_day_cell(cal, day_1_offset, day).contains(pt) {
                    if let Some(d) = NaiveDate::from_ymd_opt(
                        self.start_date.nav_year,
                        self.start_date.nav_month,
                        day,
                    ) {
                        self.start_date.value = d;
                    }
                    self.cal_open = false;
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }

            // Click outside calendar closes it
            if !cal.contains(pt) {
                self.cal_open = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Date trigger button — toggle calendar
        if Self::date_trigger_rect(panel).contains(pt) {
            self.cal_open = true;
            self.focused = Field::None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Name field click → focus
        if Self::name_field_rect(panel).contains(pt) {
            self.focused = Field::Name;
            self.cal_open = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Click anywhere else defocuses
        self.focused = Field::None;
        self.cal_open = false;
        FloatingWindowOutcome::dirty(DirtyRegion::All)
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        modifiers: &Modifiers,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if *key == Key::Named(NamedKey::Escape) {
            if self.cal_open {
                self.cal_open = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            return FloatingWindowOutcome::close();
        }
        if self.focused == Field::Name && self.name.handle_key(key, modifiers) {
            self.name_error = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        FloatingWindowOutcome::default()
    }

    fn on_paste(
        &mut self,
        text: &str,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if self.focused == Field::Name {
            self.name.handle_paste(text);
            self.name_error = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        FloatingWindowOutcome::default()
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_create = false;
        self.hovered_cancel = false;
        self.start_date.reset_hover();
    }
}
