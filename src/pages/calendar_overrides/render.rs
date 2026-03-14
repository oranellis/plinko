//! Rendering functions for the calendar overrides page.

use chrono::{Datelike, Local, NaiveDate, Weekday as CWeekday};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, RRect, Rect, TextBlob};

use crate::data::Plan;
use crate::data::schedule::chrono_to_weekday;
use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;

use super::state::CalendarOverridesState;

const SIDE_PAD: f32 = 16.0;
const CAL_HEADER_H: f32 = 36.0; // weekday header row height
const CELL_CORNER: f32 = 4.0;
const CELL_GAP: f32 = 4.0;
const USER_SEL_H: f32 = 34.0; // height of the user selector bar
const USER_TAB_PAD_X: f32 = 12.0; // horizontal padding inside each tab
const USER_TAB_H: f32 = 26.0; // pill height
const USER_TAB_GAP: f32 = 6.0; // gap between tabs
const USER_TAB_CORNER: f32 = 6.0;

/// Left arrow path (previous month button).
fn build_left_arrow(w: f32, h: f32) -> skia_safe::Path {
    let mut pb = skia_safe::PathBuilder::new();
    pb.move_to((w * 0.7, h * 0.2));
    pb.line_to((w * 0.3, h * 0.5));
    pb.line_to((w * 0.7, h * 0.8));
    pb.detach()
}

/// Right arrow path (next month button).
fn build_right_arrow(w: f32, h: f32) -> skia_safe::Path {
    let mut pb = skia_safe::PathBuilder::new();
    pb.move_to((w * 0.3, h * 0.2));
    pb.line_to((w * 0.7, h * 0.5));
    pb.line_to((w * 0.3, h * 0.8));
    pb.detach()
}

/// Days in a given month.
fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    (next.unwrap() - NaiveDate::from_ymd_opt(year, month, 1).unwrap()).num_days() as u32
}

/// 0-based weekday offset for the first day of the month (Mon=0 … Sun=6).
fn first_weekday_offset(year: i32, month: u32) -> u32 {
    let d = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    d.weekday().num_days_from_monday()
}

/// Y coordinate at which the calendar content starts (after toolbar + user selector).
fn content_top() -> f32 {
    TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + USER_SEL_H
}

/// Cell dimensions given window width/height.
fn cell_size(width: f32, height: f32, rows: u32) -> (f32, f32) {
    let avail_w = width - 2.0 * SIDE_PAD;
    let cell_w = (avail_w - 6.0 * CELL_GAP) / 7.0;
    let avail_h = height - content_top() - CAL_HEADER_H - SIDE_PAD;
    let cell_h = ((avail_h - (rows as f32 - 1.0) * CELL_GAP) / rows as f32).max(40.0);
    (cell_w, cell_h)
}

/// Grid origin (top-left of the first cell slot).
fn grid_origin() -> (f32, f32) {
    let gx = SIDE_PAD;
    let gy = content_top() + CAL_HEADER_H;
    (gx, gy)
}

/// Returns the Rect for a given grid column (0–6) and row (0–5).
fn cell_rect(col: u32, row: u32, width: f32, height: f32, rows: u32) -> Rect {
    let (cell_w, cell_h) = cell_size(width, height, rows);
    let (gx, gy) = grid_origin();
    let x = gx + col as f32 * (cell_w + CELL_GAP);
    let y = gy + row as f32 * (cell_h + CELL_GAP);
    Rect::from_xywh(x, y, cell_w, cell_h)
}

/// Returns the grid row count for the given year/month.
pub fn grid_rows(year: i32, month: u32) -> u32 {
    let dim = days_in_month(year, month);
    let offset = first_weekday_offset(year, month);
    (dim + offset).div_ceil(7).clamp(4, 6)
}

/// Returns (col, row) for a day number (1-based) in the month grid.
fn day_grid_pos(day: u32, year: i32, month: u32) -> (u32, u32) {
    let offset = first_weekday_offset(year, month);
    let idx = offset + day - 1;
    (idx % 7, idx / 7)
}

/// Build the ordered list of user tab labels: ("Plan", None) then (name, Some(id)) per user.
fn user_tabs(plan: &Plan) -> Vec<(String, Option<crate::data::ids::UserId>)> {
    let mut tabs = vec![("Plan".to_string(), None)];
    let mut users: Vec<_> = plan.users.values().collect();
    users.sort_by(|a, b| a.name.cmp(&b.name));
    for u in users {
        tabs.push((u.name.clone(), Some(u.id)));
    }
    tabs
}

/// Returns the Rect for a user selector tab using measured font widths (for drawing).
fn tab_rect(label: &str, cache: &RenderCache, tab_x: f32) -> (Rect, f32) {
    let text_w = if let Some(blob) = TextBlob::new(label, &cache.small_font) {
        blob.bounds().width()
    } else {
        label.len() as f32 * 7.0
    };
    tab_rect_with_width(text_w, tab_x)
}

/// Returns the Rect for a user selector tab using a fixed char-width estimate (for hit-testing).
fn tab_rect_approx(label: &str, tab_x: f32) -> (Rect, f32) {
    let text_w = label.len() as f32 * 7.0;
    tab_rect_with_width(text_w, tab_x)
}

fn tab_rect_with_width(text_w: f32, tab_x: f32) -> (Rect, f32) {
    let tab_w = text_w + USER_TAB_PAD_X * 2.0;
    let sel_y = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + (USER_SEL_H - USER_TAB_H) / 2.0;
    let rect = Rect::from_xywh(tab_x, sel_y, tab_w, USER_TAB_H);
    (rect, tab_x + tab_w + USER_TAB_GAP)
}

pub fn draw_calendar_overrides(
    canvas: &Canvas,
    width: f32,
    height: f32,
    state: &CalendarOverridesState,
    cache: &RenderCache,
    plan: &Plan,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Background
    paint.set_color(Color::from(GANTT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, width, height), &paint);

    draw_toolbar(canvas, state, cache, width);
    draw_user_selector(canvas, state, cache, plan, width);
    draw_month_grid(canvas, width, height, state, cache, plan);
}

fn draw_toolbar(canvas: &Canvas, state: &CalendarOverridesState, cache: &RenderCache, width: f32) {
    let left_arrow = build_left_arrow(TOOLBAR_BTN_SIZE, TOOLBAR_BTN_SIZE);
    let right_arrow = build_right_arrow(TOOLBAR_BTN_SIZE, TOOLBAR_BTN_SIZE);

    // Prev month button (0)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(0),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(0),
        &left_arrow,
    );
    // Next month button (1)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(1),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(1),
        &right_arrow,
    );

    // Month + year label centred in toolbar
    let label = format!("{} {}", month_name(state.month), state.year);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(GANTT_HEADER_MONTH_FG));
    paint.set_style(PaintStyle::Fill);
    if let Some(blob) = TextBlob::new(&label, &cache.font) {
        let bounds = blob.bounds();
        let lx = (width - bounds.width()) / 2.0 - bounds.left();
        let (_, m) = cache.font.metrics();
        let ly = TOOLBAR_BTN_Y + (TOOLBAR_BTN_SIZE - (m.descent - m.ascent)) / 2.0 - m.ascent;
        canvas.draw_text_blob(&blob, (lx, ly), &paint);
    }

    // Settings button (rightmost)
    icon_button::draw_icon_button(
        canvas,
        settings_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(2),
        &cache.icon_settings,
    );
}

fn draw_user_selector(
    canvas: &Canvas,
    state: &CalendarOverridesState,
    cache: &RenderCache,
    plan: &Plan,
    width: f32,
) {
    let tabs = user_tabs(plan);
    let sel_bg_y = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Separator bar under toolbar
    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, sel_bg_y, width, USER_SEL_H), &paint);

    let mut x = SIDE_PAD;
    for (i, (label, uid)) in tabs.iter().enumerate() {
        let (rect, next_x) = tab_rect(label, cache, x);
        x = next_x;

        let is_selected = uid.as_ref() == state.selected_user.as_ref();
        let is_hovered = state.hovered_user_tab == Some(i as i32);

        let bg = if is_selected {
            0xff_4a90d9 // accent blue
        } else if is_hovered {
            0xff_3a3a3a
        } else {
            0xff_2a2a2a
        };
        let fg = if is_selected {
            0xff_ffffff
        } else {
            0xff_cccccc
        };

        let rrect = RRect::new_rect_xy(rect, USER_TAB_CORNER, USER_TAB_CORNER);
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);

        if let Some(blob) = TextBlob::new(label.as_str(), &cache.small_font) {
            let bounds = blob.bounds();
            let (_, m) = cache.small_font.metrics();
            let tx = rect.left() + (rect.width() - bounds.width()) / 2.0 - bounds.left();
            let ty = rect.top() + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(fg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }
}

fn draw_month_grid(
    canvas: &Canvas,
    width: f32,
    height: f32,
    state: &CalendarOverridesState,
    cache: &RenderCache,
    plan: &Plan,
) {
    let rows = grid_rows(state.year, state.month);
    let today = Local::now().date_naive();

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Weekday header row
    let top_content = content_top();
    let (cell_w, _cell_h) = cell_size(width, height, rows);
    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    paint.set_color(Color::from(CAL_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, top_content, width, CAL_HEADER_H),
        &paint,
    );
    for (col, name) in day_names.iter().enumerate() {
        if let Some(blob) = TextBlob::new(*name, &cache.small_font) {
            let bounds = blob.bounds();
            let x = SIDE_PAD + col as f32 * (cell_w + CELL_GAP) + (cell_w - bounds.width()) / 2.0
                - bounds.left();
            let (_, m) = cache.small_font.metrics();
            let y = top_content + (CAL_HEADER_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(CAL_FG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_text_blob(&blob, (x, y), &paint);
        }
    }

    let dim = days_in_month(state.year, state.month);

    // Resolve which calendar to display.
    let user_cal = state
        .selected_user
        .as_ref()
        .and_then(|uid| plan.user_calendars.get(uid));

    for day in 1..=dim {
        let date = NaiveDate::from_ymd_opt(state.year, state.month, day).unwrap();
        let (col, row) = day_grid_pos(day, state.year, state.month);
        let rect = cell_rect(col, row, width, height, rows);

        // Effective hours for color coding: per-user override > plan override > schedule.
        let effective_override = user_cal
            .and_then(|c| c.get(date))
            .or_else(|| plan.calendar.get(date));

        let wd = chrono_to_weekday(date.weekday());
        let sched_hours = if let Some(uid) = &state.selected_user {
            plan.schedule_for(uid).hours_on(wd)
        } else {
            plan.default_schedule.hours_on(wd)
        };

        let bg = if let Some(override_h) = effective_override {
            if override_h == 0.0 {
                CAL_HOLIDAY_BG
            } else {
                CAL_PARTIAL_BG
            }
        } else if sched_hours == 0.0 {
            CAL_NONWORK_BG
        } else {
            CAL_WORK_BG
        };
        let hovered = state.hovered_date == Some(date);
        let final_bg = if hovered { CAL_HOVER_BG } else { bg };

        let rrect = RRect::new_rect_xy(rect, CELL_CORNER, CELL_CORNER);
        paint.set_color(Color::from(final_bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);

        // Today border
        if date == today {
            paint.set_color(Color::from(CAL_TODAY_BORDER));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(2.0);
            canvas.draw_rrect(rrect, &paint);
        } else {
            paint.set_color(Color::from(CAL_CELL_BORDER));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.0);
            canvas.draw_rrect(rrect, &paint);
        }

        // Day number
        let day_str = day.to_string();
        if let Some(blob) = TextBlob::new(&day_str, &cache.small_font) {
            let (_, m) = cache.small_font.metrics();
            let ty = rect.top() + 4.0 - m.ascent;
            paint.set_color(Color::from(CAL_FG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_text_blob(&blob, (rect.left() + 6.0, ty), &paint);
        }

        // Override hours label — show user-specific override if set, else plan override.
        let display_h = user_cal
            .and_then(|c| c.get(date))
            .or_else(|| plan.calendar.get(date));
        if let Some(h) = display_h {
            // Show a "U" indicator if this is a user-specific override.
            let user_specific = user_cal.and_then(|c| c.get(date)).is_some();
            let h_str = if user_specific {
                format!("{h:.1}h ★")
            } else {
                format!("{h:.1}h")
            };
            if let Some(blob) = TextBlob::new(&h_str, &cache.small_font) {
                let bounds = blob.bounds();
                let (_, m) = cache.small_font.metrics();
                let tx = rect.left() + (rect.width() - bounds.width()) / 2.0 - bounds.left();
                let ty = rect.bottom() - 6.0 - (m.descent - m.ascent) - m.ascent;
                paint.set_color(Color::from(0xff_666666));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
        }

        // Inline edit popup
        if state.editing_date == Some(date) {
            draw_edit_popup(canvas, rect, state, cache);
        }
    }
}

fn draw_edit_popup(
    canvas: &Canvas,
    cell: Rect,
    state: &CalendarOverridesState,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    let popup_w = cell.width().max(120.0);
    let popup_h = 48.0;
    let popup_x = cell.left();
    let popup_y = cell.top() - popup_h - 4.0;

    let popup_rect = Rect::from_xywh(popup_x, popup_y, popup_w, popup_h);
    let popup_rrect = RRect::new_rect_xy(popup_rect, 4.0, 4.0);

    // Shadow / background
    let border_color = if state.edit_error {
        0xff_cc3333
    } else {
        CAL_TODAY_BORDER
    };
    paint.set_color(Color::from(0xff_ffffff));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(popup_rrect, &paint);
    paint.set_color(Color::from(border_color));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    canvas.draw_rrect(popup_rrect, &paint);

    // Input text
    let display = if state.edit_input.is_empty() {
        "hours…"
    } else {
        &state.edit_input
    };
    let text_color = if state.edit_input.is_empty() {
        CAL_DIM_FG
    } else {
        CAL_FG
    };
    if let Some(blob) = TextBlob::new(display, &cache.small_font) {
        let (_, m) = cache.small_font.metrics();
        let ty = popup_y + (popup_h - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(Color::from(text_color));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_text_blob(&blob, (popup_x + 8.0, ty), &paint);
    }

    // Hint
    if let Some(blob) = TextBlob::new("Enter set  Esc cancel", &cache.small_font) {
        let bounds = blob.bounds();
        let (_, m) = cache.small_font.metrics();
        let tx = popup_x + popup_w - bounds.width() - 6.0 - bounds.left();
        let ty = popup_y + popup_h - 4.0 - m.descent;
        paint.set_color(Color::from(CAL_DIM_FG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }
}

/// Hit-test a day cell from mouse position. Returns `NaiveDate` if hit.
pub fn hit_test_day(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    year: i32,
    month: u32,
) -> Option<NaiveDate> {
    let rows = grid_rows(year, month);
    let dim = days_in_month(year, month);
    for day in 1..=dim {
        let (col, row) = day_grid_pos(day, year, month);
        let rect = cell_rect(col, row, width, height, rows);
        if x >= rect.left() && x <= rect.right() && y >= rect.top() && y <= rect.bottom() {
            return NaiveDate::from_ymd_opt(year, month, day);
        }
    }
    None
}

pub fn hit_test_toolbar_buttons(x: f32, y: f32, width: f32) -> Option<usize> {
    let s = TOOLBAR_BTN_SIZE;
    let btns = [
        (toolbar_btn_x(0), TOOLBAR_BTN_Y),
        (toolbar_btn_x(1), TOOLBAR_BTN_Y),
        (settings_btn_x(width), TOOLBAR_BTN_Y),
    ];
    for (i, (bx, by)) in btns.iter().enumerate() {
        if x >= *bx && x <= *bx + s && y >= *by && y <= *by + s {
            return Some(i);
        }
    }
    None
}

/// Hit-test the user selector tabs. Returns tab index if a tab was hit.
/// Uses a fixed character-width estimate so it works without font access.
pub fn hit_test_user_tab(x: f32, y: f32, plan: &Plan) -> Option<usize> {
    let tabs = user_tabs(plan);
    let sel_top = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + (USER_SEL_H - USER_TAB_H) / 2.0;
    let sel_bot = sel_top + USER_TAB_H;
    if y < sel_top || y > sel_bot {
        return None;
    }
    let mut tab_x = SIDE_PAD;
    for (i, (label, _)) in tabs.iter().enumerate() {
        let (rect, next_x) = tab_rect_approx(label, tab_x);
        tab_x = next_x;
        if x >= rect.left() && x <= rect.right() {
            return Some(i);
        }
    }
    None
}

/// Returns the `Option<UserId>` for the tab at the given index.
pub fn user_for_tab_index(idx: usize, plan: &Plan) -> Option<crate::data::ids::UserId> {
    user_tabs(plan)
        .into_iter()
        .nth(idx)
        .and_then(|(_, uid)| uid)
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}
