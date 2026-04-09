//! Floating window for editing top-level plan settings.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, BTN_PRIMARY_BG, BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG,
    BTN_SECONDARY_FG, CAL_SELECTED_BG, DEP_PLAN_START_FG, DIVIDER_COLOR, INPUT_BG, INPUT_BORDER,
    INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG,
    LIST_BG, LIST_ITEM_HOVER_BG, MUTED_FG, OVERLAY_LIGHT, OVERLAY_SOFT, OVERLAY_XLIGHT, PANEL_BG,
    PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP,
    SUBTLE_FG, TOOLBAR_BTN_HOVER_BG,
};
use crate::ui::text_input::TextInput;
use plinko_shared::data::Plan;
use plinko_shared::data::ids::NodeId;
use plinko_shared::protocol::PlanRequest;

// ── Layout constants ──────────────────────────────────────────────────────────

const PANEL_W: f32 = 520.0;
const PANEL_H: f32 = 480.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const SAVE_BTN_W: f32 = 80.0;
const CANCEL_BTN_W: f32 = 80.0;
const LABEL_H: f32 = 14.0;
const EDIT_SCHEDULE_BTN_W: f32 = 130.0;

// Calendar popup
const CAL_PAD: f32 = 8.0;
const CAL_CELL: f32 = 32.0;
const CAL_W: f32 = CAL_CELL * 7.0 + CAL_PAD * 2.0;
const CAL_HEADER_H: f32 = 28.0;
const CAL_DOW_H: f32 = 20.0;
const CAL_ROW_H: f32 = 26.0;
const CAL_FOOTER_H: f32 = 28.0;
const CAL_H: f32 = CAL_PAD + CAL_HEADER_H + CAL_DOW_H + CAL_ROW_H * 6.0 + CAL_FOOTER_H + CAL_PAD;

// Target dropdown
const TARGET_DD_FILTER_H: f32 = PLAN_INPUT_H;
const TARGET_DD_ROW_H: f32 = 28.0;
const MAX_TARGET_DD_ROWS: usize = 5;
const TARGET_DD_H: f32 = TARGET_DD_FILTER_H + MAX_TARGET_DD_ROWS as f32 * TARGET_DD_ROW_H;

// ── CalendarPicker ────────────────────────────────────────────────────────────

struct CalendarPicker {
    value: Option<NaiveDate>,
    nav_year: i32,
    nav_month: u32,
    hovered_day: Option<u32>,
    hovered_prev_year: bool,
    hovered_prev_month: bool,
    hovered_next_month: bool,
    hovered_next_year: bool,
    hovered_clear: bool,
    hovered_today: bool,
    hovered_trigger: bool,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl CalendarPicker {
    fn new(value: Option<NaiveDate>) -> Self {
        let base = value.unwrap_or_else(|| chrono::Local::now().date_naive());
        Self {
            value,
            nav_year: base.year(),
            nav_month: base.month(),
            hovered_day: None,
            hovered_prev_year: false,
            hovered_prev_month: false,
            hovered_next_month: false,
            hovered_next_year: false,
            hovered_clear: false,
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
        self.hovered_clear = false;
        self.hovered_today = false;
        self.hovered_trigger = false;
    }

    fn display_text(&self) -> String {
        match self.value {
            Some(d) => format!("{}", d.format("%d %b %Y")),
            None => "—".to_string(),
        }
    }
}
// }}}

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

// ── Calendar nav button rects (free functions, mirroring task_form_window) ────

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

fn cal_clear_btn(cal: Rect) -> Rect {
    let w = 48.0;
    let h = 22.0;
    Rect::from_xywh(cal.right - CAL_PAD - w, cal.bottom - CAL_PAD - h, w, h)
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

// ── Draw helpers ──────────────────────────────────────────────────────────────

fn draw_text_input(
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

fn draw_date_btn(
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
        paint.set_color(if picker.value.is_some() {
            Color::from(INPUT_FG)
        } else {
            Color::from(MUTED_FG)
        });
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Tiny calendar icon
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

fn draw_calendar_popup(
    canvas: &Canvas,
    cal: Rect,
    picker: &CalendarPicker,
    today: NaiveDate,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

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

    let day_1_offset = first_weekday_offset(picker.nav_year, picker.nav_month);
    let num_days = days_in_month(picker.nav_year, picker.nav_month);
    let today_in_nav = today.year() == picker.nav_year && today.month() == picker.nav_month;

    for day in 1..=num_days {
        let cell = cal_day_cell(cal, day_1_offset, day);
        let is_selected = picker
            .value
            .map(|v| v.year() == picker.nav_year && v.month() == picker.nav_month && v.day() == day)
            .unwrap_or(false);
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

    let footer_y = cal.bottom - CAL_PAD - CAL_FOOTER_H;
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(cal.left, footer_y, cal.width(), 1.0),
        &paint,
    );

    let clear_btn = cal_clear_btn(cal);
    paint.set_color(Color::from(if picker.hovered_clear {
        TOOLBAR_BTN_HOVER_BG
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(clear_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Clear", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Clear", None);
        let tx = clear_btn.left + (clear_btn.width() - adv) / 2.0;
        let ty = clear_btn.top + (clear_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

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

fn draw_target_trigger_btn(
    canvas: &Canvas,
    rect: Rect,
    label: &str,
    is_open: bool,
    is_plan_start: bool,
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
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    if let Some(blob) = TextBlob::new(label, &cache.font) {
        let (_, m) = cache.font.metrics();
        let ty = rect.top + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(Color::from(if is_plan_start {
            DEP_PLAN_START_FG
        } else {
            INPUT_FG
        }));
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Dropdown chevron
    let cx = rect.right - 14.0;
    let cy = rect.top + rect.height() / 2.0;
    let s = 3.5;
    paint.set_color(Color::from(SUBTLE_FG));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    let mut pb = PathBuilder::new();
    if is_open {
        pb.move_to((cx - s, cy + s * 0.5));
        pb.line_to((cx, cy - s * 0.5));
        pb.line_to((cx + s, cy + s * 0.5));
    } else {
        pb.move_to((cx - s, cy - s * 0.5));
        pb.line_to((cx, cy + s * 0.5));
        pb.line_to((cx + s, cy - s * 0.5));
    }
    canvas.draw_path(&pb.detach(), &paint);
}

#[allow(clippy::too_many_arguments)]
fn draw_target_dropdown(
    canvas: &Canvas,
    dd: Rect,
    filter: &TextInput,
    selected: NodeId,
    hovered_row: Option<usize>,
    scroll: usize,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    // Filter input
    let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), TARGET_DD_FILTER_H);
    draw_text_input(canvas, filter_rect, filter, true, false, cache);

    // Divider
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + TARGET_DD_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    let f = filter.content.to_lowercase();
    let items = build_target_items(&f, plan);

    let list_top = dd.top + TARGET_DD_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        dd.height() - TARGET_DD_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if items.is_empty() {
        if let Some(blob) = TextBlob::new("No matches", &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (dd.left + 8.0, list_top + 8.0 - sm.ascent), &paint);
        }
    } else {
        let end = (scroll + MAX_TARGET_DD_ROWS).min(items.len());
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        for (vis, (node_id, name)) in items[scroll..end].iter().enumerate() {
            let abs = scroll + vis;
            let ry = list_top + vis as f32 * TARGET_DD_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), TARGET_DD_ROW_H);

            if hovered_row == Some(abs) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            // Tick if selected
            if *node_id == selected {
                let tx = dd.left + 10.0;
                let ty = ry + TARGET_DD_ROW_H / 2.0;
                paint.set_color(Color::from(BTN_PRIMARY_BG));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.5);
                let mut pb = PathBuilder::new();
                pb.move_to((tx, ty));
                pb.line_to((tx + 3.0, ty + 3.0));
                pb.line_to((tx + 7.0, ty - 3.0));
                canvas.draw_path(&pb.detach(), &paint);
                paint.set_style(PaintStyle::Fill);
            }

            if let Some(blob) = TextBlob::new(name, &cache.small_font) {
                let ty = ry + (TARGET_DD_ROW_H - sm_h) / 2.0 - sm.ascent;
                let fg = if *node_id == NodeId::PlanStart {
                    DEP_PLAN_START_FG
                } else {
                    ITEM_FG
                };
                paint.set_color(Color::from(fg));
                canvas.draw_text_blob(&blob, (dd.left + 22.0, ty), &paint);
            }
        }
    }

    canvas.restore();
}

fn build_target_items(filter: &str, plan: &Plan) -> Vec<(NodeId, String)> {
    let mut items: Vec<(NodeId, String)> = Vec::new();
    if filter.is_empty() || "plan start".contains(filter) {
        items.push((NodeId::PlanStart, "Plan Start".to_string()));
    }
    let mut task_items: Vec<(NodeId, String)> = plan
        .tasks
        .iter()
        .filter(|(_, t)| filter.is_empty() || t.name.to_lowercase().contains(filter))
        .map(|(id, t)| (NodeId::Task(*id), t.name.clone()))
        .collect();
    task_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(task_items);
    let mut ms_items: Vec<(NodeId, String)> = plan
        .milestones
        .iter()
        .filter(|(_, m)| filter.is_empty() || m.name.to_lowercase().contains(filter))
        .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone()))
        .collect();
    ms_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(ms_items);
    items
}

fn node_display_name(node: NodeId, plan: &Plan) -> String {
    match node {
        NodeId::PlanStart => "Plan Start".to_string(),
        NodeId::Task(id) => plan
            .tasks
            .get(&id)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "Unknown Task".to_string()),
        NodeId::Milestone(id) => plan
            .milestones
            .get(&id)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "Unknown Milestone".to_string()),
    }
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct PlanSettingsWindow {
    name: TextInput,
    start_date: CalendarPicker,
    calendar_open: bool,
    target_filter: TextInput,
    target_dropdown_open: bool,
    target_dropdown_hovered: Option<usize>,
    target_dropdown_scroll: usize,
    selected_target: NodeId,
    hovered_back: bool,
    hovered_save: bool,
    hovered_cancel: bool,
    hovered_edit_schedule: bool,
    scroll_y: f32,
    error: Option<String>,
    pending_schedule: Option<Box<dyn crate::ui::floating_window::FloatingWindow>>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl PlanSettingsWindow {
    pub fn new(plan: &Plan) -> Self {
        let mut name = TextInput::new(&plan.name);
        name.focused = true;
        Self {
            name,
            start_date: CalendarPicker::new(Some(plan.start_date)),
            calendar_open: false,
            target_filter: TextInput::new(""),
            target_dropdown_open: false,
            target_dropdown_hovered: None,
            target_dropdown_scroll: 0,
            selected_target: plan.scheduler_target,
            hovered_back: false,
            hovered_save: false,
            hovered_cancel: false,
            hovered_edit_schedule: false,
            scroll_y: 0.0,
            error: None,
            pending_schedule: None,
        }
    }

    /// Construct from pre-extracted plan data (used by callers without direct Plan access).
    pub fn with_values(name: &str, date_str: &str, scheduler_target: NodeId) -> Self {
        let date = date_str.parse::<NaiveDate>().ok();
        let mut name_input = TextInput::new(name);
        name_input.focused = true;
        Self {
            name: name_input,
            start_date: CalendarPicker::new(date),
            calendar_open: false,
            target_filter: TextInput::new(""),
            target_dropdown_open: false,
            target_dropdown_hovered: None,
            target_dropdown_scroll: 0,
            selected_target: scheduler_target,
            hovered_back: false,
            hovered_save: false,
            hovered_cancel: false,
            hovered_edit_schedule: false,
            scroll_y: 0.0,
            error: None,
            pending_schedule: None,
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

    fn content_w(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).width() - 2.0 * PLAN_FORM_PADDING
    }

    /// Y of the first content row's top (absolute, before scroll).
    fn content_start_y(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn name_input_rect(width: f32, height: f32) -> Rect {
        let x = Self::content_x(width, height);
        let w = Self::content_w(width, height);
        let y = Self::content_start_y(width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn start_date_input_rect(width: f32, height: f32) -> Rect {
        let r = Self::name_input_rect(width, height);
        let y = r.bottom + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(r.left, y, r.width(), PLAN_INPUT_H)
    }

    fn target_input_rect(width: f32, height: f32) -> Rect {
        let r = Self::start_date_input_rect(width, height);
        let y = r.bottom + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(r.left, y, r.width(), PLAN_INPUT_H)
    }

    fn edit_schedule_btn_rect(width: f32, height: f32) -> Rect {
        let r = Self::target_input_rect(width, height);
        let y = r.bottom + PLAN_FIELD_GAP;
        let x = Self::content_x(width, height);
        Rect::from_xywh(x, y, EDIT_SCHEDULE_BTN_W, PLAN_BTN_H)
    }

    fn max_scroll(width: f32, height: f32) -> f32 {
        let panel = Self::panel_rect(width, height);
        let content_bottom = Self::edit_schedule_btn_rect(width, height).bottom + PLAN_FORM_PADDING;
        let content_top = Self::content_start_y(width, height);
        let full_content_h = content_bottom - content_top;
        // visible height = panel height minus title, minus bottom buttons section
        let visible_h =
            panel.height() - TITLE_H - 1.0 - PLAN_FORM_PADDING - PLAN_BTN_H - PLAN_FORM_PADDING;
        (full_content_h - visible_h).max(0.0)
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

    /// Convert a screen-space Y to content-space Y (accounting for scroll).
    fn to_content_y(&self, y: f32) -> f32 {
        y + self.scroll_y
    }

    /// Dropdown position (screen space, below/above the scrolled trigger).
    fn target_dropdown_rect(&self, width: f32, height: f32) -> Rect {
        let trigger = Self::target_input_rect(width, height);
        let trigger_screen_top = trigger.top - self.scroll_y;
        let trigger_screen = Rect::from_xywh(
            trigger.left,
            trigger_screen_top,
            trigger.width(),
            trigger.height(),
        );
        let panel = Self::panel_rect(width, height);
        let save_top = Self::save_btn_rect(width, height).top;
        let below = trigger_screen.bottom + 4.0;
        let above = trigger_screen.top - 4.0 - TARGET_DD_H;
        let top = if below + TARGET_DD_H <= save_top - PLAN_FORM_PADDING {
            below
        } else {
            above
        };
        let left = trigger_screen.left.max(panel.left + 4.0);
        Rect::from_xywh(left, top, trigger_screen.width(), TARGET_DD_H)
    }

    fn try_save(&self, sender: &PlanRequestSender) -> Result<(), String> {
        let name = self.name.content.trim().to_string();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        let date = self
            .start_date
            .value
            .ok_or_else(|| "Start date is required".to_string())?;
        sender.send(PlanRequest::UpdatePlanSettings {
            name,
            start_date: date,
            scheduler_target: self.selected_target,
        });
        Ok(())
    }

    fn close_target_dropdown(&mut self) {
        self.target_dropdown_open = false;
        self.target_dropdown_hovered = None;
        self.target_filter = TextInput::new("");
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl FloatingWindow for PlanSettingsWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Shadow
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
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Title bar background (rounded top corners only)
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

        // Title text (centred in title bar)
        let title = "Plan Settings";
        paint.set_color(Color::from(ITEM_FG));
        if let Some(blob) = TextBlob::new(title, &cache.font) {
            let (adv, _) = cache.font.measure_str(title, None);
            let (_, m) = cache.font.metrics();
            let tx = panel.left + (panel.width() - adv) / 2.0;
            let ty = panel.top + (TITLE_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Back chevron button
        let back_rect = Self::back_btn_rect(width, height);
        crate::ui::window_chrome::draw_chevron_btn(canvas, back_rect, self.hovered_back);

        // Divider below title bar
        paint.set_color(Color::from(DIVIDER_COLOR));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // ── Bottom bar (Save / Cancel) — not scrolled ─────────────────────────
        let save_rect = Self::save_btn_rect(width, height);
        let cancel_rect = Self::cancel_btn_rect(width, height);

        // Separator above buttons
        let sep_y = save_rect.top - PLAN_FORM_PADDING;
        paint.set_color(Color::from(DIVIDER_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line((panel.left, sep_y), (panel.right, sep_y), &paint);
        paint.set_style(PaintStyle::Fill);

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
            let (adv, _) = cache.font.measure_str("Save", None);
            let (_, fm) = cache.font.metrics();
            let tx = save_rect.left + (save_rect.width() - adv) / 2.0;
            let ty =
                save_rect.top + (save_rect.height() - (fm.descent - fm.ascent)) / 2.0 - fm.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        let cancel_bg = if self.hovered_cancel {
            TOOLBAR_BTN_HOVER_BG
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
            let (adv, _) = cache.font.measure_str("Cancel", None);
            let (_, fm) = cache.font.metrics();
            let tx = cancel_rect.left + (cancel_rect.width() - adv) / 2.0;
            let ty = cancel_rect.top + (cancel_rect.height() - (fm.descent - fm.ascent)) / 2.0
                - fm.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // ── Scrollable content area ───────────────────────────────────────────
        let content_clip =
            Rect::from_ltrb(panel.left, panel.top + TITLE_H + 1.0, panel.right, sep_y);
        canvas.save();
        canvas.clip_rect(content_clip, ClipOp::Intersect, false);
        canvas.translate((0.0, -self.scroll_y));

        let lx = Self::content_x(width, height);

        // Name field
        let name_rect = Self::name_input_rect(width, height);
        paint.set_color(Color::from(LABEL_FG));
        if let Some(blob) = TextBlob::new("Plan Name", &cache.small_font) {
            canvas.draw_text_blob(&blob, (lx, name_rect.top - PLAN_LABEL_GAP), &paint);
        }
        draw_text_input(
            canvas,
            name_rect,
            &self.name,
            self.name.focused,
            false,
            cache,
        );

        // Start Date field
        let date_rect = Self::start_date_input_rect(width, height);
        paint.set_color(Color::from(LABEL_FG));
        if let Some(blob) = TextBlob::new("Start Date", &cache.small_font) {
            canvas.draw_text_blob(&blob, (lx, date_rect.top - PLAN_LABEL_GAP), &paint);
        }
        draw_date_btn(
            canvas,
            date_rect,
            &self.start_date,
            self.calendar_open,
            cache,
        );

        // Scheduler Target field
        let target_rect = Self::target_input_rect(width, height);
        paint.set_color(Color::from(LABEL_FG));
        if let Some(blob) = TextBlob::new("Scheduler Target", &cache.small_font) {
            canvas.draw_text_blob(&blob, (lx, target_rect.top - PLAN_LABEL_GAP), &paint);
        }
        let target_label = node_display_name(self.selected_target, plan);
        let is_plan_start = self.selected_target == NodeId::PlanStart;
        draw_target_trigger_btn(
            canvas,
            target_rect,
            &target_label,
            self.target_dropdown_open,
            is_plan_start,
            cache,
        );

        // Edit Schedule placeholder button
        let edit_rect = Self::edit_schedule_btn_rect(width, height);
        let edit_bg = if self.hovered_edit_schedule {
            TOOLBAR_BTN_HOVER_BG
        } else {
            BTN_SECONDARY_BG
        };
        paint.set_color(Color::from(edit_bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(edit_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        if let Some(blob) = TextBlob::new("Edit Schedule", &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str("Edit Schedule", None);
            let (_, sm) = cache.small_font.metrics();
            let tx = edit_rect.left + (edit_rect.width() - adv) / 2.0;
            let ty =
                edit_rect.top + (edit_rect.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Error message
        if let Some(err) = &self.error {
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            if let Some(blob) = TextBlob::new(err.as_str(), &cache.small_font) {
                let (_, sm) = cache.small_font.metrics();
                canvas.draw_text_blob(&blob, (lx, edit_rect.bottom + 8.0 - sm.ascent), &paint);
            }
        }

        canvas.restore(); // end content scroll

        // Scrollbar
        let sep_y = Self::save_btn_rect(width, height).top - PLAN_FORM_PADDING;
        let visible_h = sep_y - (panel.top + TITLE_H + 1.0);
        let total_h = PANEL_H - TITLE_H - 1.0 - PLAN_BTN_H - PLAN_FORM_PADDING * 2.0;
        crate::ui::window_chrome::draw_window_scrollbar(
            canvas,
            panel.right,
            panel.top + TITLE_H + 1.0,
            visible_h,
            total_h,
            self.scroll_y,
        );

        // ── Overlays: calendar and target dropdown (above everything) ─────────
        let today = chrono::Local::now().date_naive();

        if self.calendar_open {
            let date_abs = Self::start_date_input_rect(width, height);
            let date_screen = Rect::from_xywh(
                date_abs.left,
                date_abs.top - self.scroll_y,
                date_abs.width(),
                date_abs.height(),
            );
            let cal_rect = calendar_popup_rect(date_screen, panel);
            draw_calendar_popup(canvas, cal_rect, &self.start_date, today, cache);
        }

        if self.target_dropdown_open {
            let dd = self.target_dropdown_rect(width, height);
            draw_target_dropdown(
                canvas,
                dd,
                &self.target_filter,
                self.selected_target,
                self.target_dropdown_hovered,
                self.target_dropdown_scroll,
                plan,
                cache,
            );
        }
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> FloatingWindowOutcome {
        let p = Point::new(x, y);
        let cy = self.to_content_y(y); // content-space y

        let mut dirty = false;

        // Static buttons (back, save, cancel)
        let hb = Self::back_btn_rect(width, height).contains(p);
        let hs = Self::save_btn_rect(width, height).contains(p);
        let hc = Self::cancel_btn_rect(width, height).contains(p);
        if hb != self.hovered_back || hs != self.hovered_save || hc != self.hovered_cancel {
            self.hovered_back = hb;
            self.hovered_save = hs;
            self.hovered_cancel = hc;
            dirty = true;
        }

        // Edit Schedule button (content-space)
        let edit_rect = Self::edit_schedule_btn_rect(width, height);
        let hes = edit_rect.contains(Point::new(x, cy));
        if hes != self.hovered_edit_schedule {
            self.hovered_edit_schedule = hes;
            dirty = true;
        }

        // Calendar hover
        if self.calendar_open {
            let date_abs = Self::start_date_input_rect(width, height);
            let date_screen = Rect::from_xywh(
                date_abs.left,
                date_abs.top - self.scroll_y,
                date_abs.width(),
                date_abs.height(),
            );
            let cal = calendar_popup_rect(date_screen, Self::panel_rect(width, height));

            let prev_hov = (
                self.start_date.hovered_day,
                self.start_date.hovered_prev_year,
                self.start_date.hovered_prev_month,
                self.start_date.hovered_next_month,
                self.start_date.hovered_next_year,
                self.start_date.hovered_clear,
                self.start_date.hovered_today,
            );
            self.start_date.reset_hover();

            if cal_prev_year_btn(cal).contains(p) {
                self.start_date.hovered_prev_year = true;
            } else if cal_prev_month_btn(cal).contains(p) {
                self.start_date.hovered_prev_month = true;
            } else if cal_next_month_btn(cal).contains(p) {
                self.start_date.hovered_next_month = true;
            } else if cal_next_year_btn(cal).contains(p) {
                self.start_date.hovered_next_year = true;
            } else if cal_clear_btn(cal).contains(p) {
                self.start_date.hovered_clear = true;
            } else if cal_today_btn(cal).contains(p) {
                self.start_date.hovered_today = true;
            } else {
                let off = first_weekday_offset(self.start_date.nav_year, self.start_date.nav_month);
                let nd = days_in_month(self.start_date.nav_year, self.start_date.nav_month);
                for day in 1..=nd {
                    if cal_day_cell(cal, off, day).contains(p) {
                        self.start_date.hovered_day = Some(day);
                        break;
                    }
                }
            }

            let new_hov = (
                self.start_date.hovered_day,
                self.start_date.hovered_prev_year,
                self.start_date.hovered_prev_month,
                self.start_date.hovered_next_month,
                self.start_date.hovered_next_year,
                self.start_date.hovered_clear,
                self.start_date.hovered_today,
            );
            if prev_hov != new_hov {
                dirty = true;
            }
        } else {
            // Trigger hover for date button
            let date_abs = Self::start_date_input_rect(width, height);
            let date_screen_top = date_abs.top - self.scroll_y;
            let date_screen = Rect::from_xywh(
                date_abs.left,
                date_screen_top,
                date_abs.width(),
                date_abs.height(),
            );
            let prev = self.start_date.hovered_trigger;
            self.start_date.hovered_trigger = date_screen.contains(p);
            if prev != self.start_date.hovered_trigger {
                dirty = true;
            }
        }

        // Dropdown hover
        if self.target_dropdown_open {
            let dd = self.target_dropdown_rect(width, height);
            let list_top = dd.top + TARGET_DD_FILTER_H + 1.0;
            let new_hov = if y >= list_top && dd.contains(p) {
                let f = self.target_filter.content.to_lowercase();
                let items = build_target_items(&f, plan);
                let row_idx =
                    ((y - list_top) / TARGET_DD_ROW_H) as usize + self.target_dropdown_scroll;
                if row_idx < items.len() {
                    Some(row_idx)
                } else {
                    None
                }
            } else {
                None
            };
            if new_hov != self.target_dropdown_hovered {
                self.target_dropdown_hovered = new_hov;
                dirty = true;
            }
        }

        if dirty {
            FloatingWindowOutcome::dirty(DirtyRegion::All)
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
        let panel = Self::panel_rect(width, height);
        let cy = self.to_content_y(y);

        // Static buttons first (not affected by scroll)
        if Self::back_btn_rect(width, height).contains(p) {
            return FloatingWindowOutcome::close();
        }
        if Self::cancel_btn_rect(width, height).contains(p) {
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

        // Calendar popup interactions
        if self.calendar_open {
            let date_abs = Self::start_date_input_rect(width, height);
            let date_screen = Rect::from_xywh(
                date_abs.left,
                date_abs.top - self.scroll_y,
                date_abs.width(),
                date_abs.height(),
            );
            let cal = calendar_popup_rect(date_screen, panel);

            if cal.contains(p) {
                if cal_prev_year_btn(cal).contains(p) {
                    self.start_date.prev_year();
                } else if cal_prev_month_btn(cal).contains(p) {
                    self.start_date.prev_month();
                } else if cal_next_month_btn(cal).contains(p) {
                    self.start_date.next_month();
                } else if cal_next_year_btn(cal).contains(p) {
                    self.start_date.next_year();
                } else if cal_clear_btn(cal).contains(p) {
                    self.start_date.value = None;
                    self.calendar_open = false;
                } else if cal_today_btn(cal).contains(p) {
                    let today = chrono::Local::now().date_naive();
                    self.start_date.value = Some(today);
                    self.start_date.nav_year = today.year();
                    self.start_date.nav_month = today.month();
                    self.calendar_open = false;
                } else {
                    let off =
                        first_weekday_offset(self.start_date.nav_year, self.start_date.nav_month);
                    let nd = days_in_month(self.start_date.nav_year, self.start_date.nav_month);
                    for day in 1..=nd {
                        if cal_day_cell(cal, off, day).contains(p) {
                            self.start_date.value = NaiveDate::from_ymd_opt(
                                self.start_date.nav_year,
                                self.start_date.nav_month,
                                day,
                            );
                            self.calendar_open = false;
                            break;
                        }
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            // Click outside calendar: close it
            self.calendar_open = false;
            if !panel.contains(p) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Target dropdown interactions
        if self.target_dropdown_open {
            let dd = self.target_dropdown_rect(width, height);
            if dd.contains(p) {
                let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), TARGET_DD_FILTER_H);
                if filter_rect.contains(p) {
                    // Click in filter box — just keep focus there
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
                let list_top = dd.top + TARGET_DD_FILTER_H + 1.0;
                if y >= list_top {
                    let abs =
                        ((y - list_top) / TARGET_DD_ROW_H) as usize + self.target_dropdown_scroll;
                    let f = self.target_filter.content.to_lowercase();
                    let items = build_target_items(&f, plan);
                    if let Some((node_id, _)) = items.get(abs) {
                        self.selected_target = *node_id;
                    }
                    self.close_target_dropdown();
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            self.close_target_dropdown();
            if !panel.contains(p) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Content-space hit tests (account for scroll)
        let cp = Point::new(x, cy);

        if Self::name_input_rect(width, height).contains(cp) {
            self.name.focused = true;
            self.error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Start date trigger (screen-space check against scrolled rect)
        let date_abs = Self::start_date_input_rect(width, height);
        let date_screen = Rect::from_xywh(
            date_abs.left,
            date_abs.top - self.scroll_y,
            date_abs.width(),
            date_abs.height(),
        );
        if date_screen.contains(p) {
            self.calendar_open = !self.calendar_open;
            self.name.focused = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Target dropdown trigger (screen-space)
        let target_abs = Self::target_input_rect(width, height);
        let target_screen = Rect::from_xywh(
            target_abs.left,
            target_abs.top - self.scroll_y,
            target_abs.width(),
            target_abs.height(),
        );
        if target_screen.contains(p) {
            self.target_dropdown_open = !self.target_dropdown_open;
            self.name.focused = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // Edit Schedule button (content-space)
        if Self::edit_schedule_btn_rect(width, height).contains(Point::new(x, cy)) {
            self.pending_schedule = Some(Box::new(
                crate::ui::schedule_window::ScheduleWindow::for_plan(&plan.default_schedule),
            ));
            return FloatingWindowOutcome::default();
        }

        // Click outside panel
        if !panel.contains(p) {
            return FloatingWindowOutcome::close();
        }

        // Defocus name if clicking elsewhere inside panel
        if self.name.focused {
            self.name.focused = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        modifiers: &Modifiers,
        sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        // Target dropdown active: route keys to filter
        if self.target_dropdown_open {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.close_target_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
                Key::Named(NamedKey::Enter) => {
                    self.close_target_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
                _ => {}
            }
            if self.target_filter.handle_key(key, modifiers) {
                self.target_dropdown_scroll = 0;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            return FloatingWindowOutcome::default();
        }

        // Calendar open: only Escape closes it
        if self.calendar_open {
            if *key == Key::Named(NamedKey::Escape) {
                self.calendar_open = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            return FloatingWindowOutcome::default();
        }

        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => match self.try_save(sender) {
                Ok(()) => FloatingWindowOutcome::close(),
                Err(e) => {
                    self.error = Some(e);
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                }
            },
            Key::Named(NamedKey::Tab) => {
                self.name.focused = !self.name.focused;
                FloatingWindowOutcome::dirty(DirtyRegion::All)
            }
            _ if self.name.focused => {
                if self.name.handle_key(key, modifiers) {
                    self.error = None;
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
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
        if self.name.focused {
            self.name.handle_paste(text);
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        if self.target_dropdown_open {
            self.target_filter.handle_paste(text);
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        FloatingWindowOutcome::default()
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        let max = Self::max_scroll(width, height);
        if max <= 0.0 {
            return FloatingWindowOutcome::default();
        }
        let new_scroll = (self.scroll_y - delta_y * 30.0).clamp(0.0, max);
        if (new_scroll - self.scroll_y).abs() > 0.1 {
            self.scroll_y = new_scroll;
            FloatingWindowOutcome::dirty(DirtyRegion::All)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
        self.hovered_cancel = false;
        self.hovered_edit_schedule = false;
        self.start_date.reset_hover();
    }

    fn take_open_request(&mut self) -> Option<Box<dyn crate::ui::floating_window::FloatingWindow>> {
        self.pending_schedule.take()
    }
}
// }}}
