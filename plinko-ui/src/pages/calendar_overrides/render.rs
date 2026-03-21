//! Rendering functions for the calendar overrides page.

use chrono::{Datelike, Local, NaiveDate, Weekday as CWeekday};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, RRect, Rect, TextBlob};

use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;
use plinko_shared::data::Plan;
use plinko_shared::data::schedule::chrono_to_weekday;

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
const DROPDOWN_ITEM_H: f32 = 32.0;
const DROPDOWN_MAX_H: f32 = 200.0;
const DROPDOWN_FILTER_H: f32 = 30.0;

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
/// Returns other users (not the current user), sorted by name.
pub fn other_users(
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
) -> Vec<&plinko_shared::data::User> {
    let mut users: Vec<_> = plan
        .users_data
        .values()
        .map(|ud| &ud.user)
        .filter(|u| current_user != Some(u.id))
        .collect();
    users.sort_by(|a, b| a.name.cmp(&b.name));
    users
}

/// Count of users other than the current user.
pub fn other_users_count(
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
) -> usize {
    plan.users_data
        .values()
        .filter(|ud| current_user != Some(ud.user.id))
        .count()
}

fn tab_rect_with_width(text_w: f32, tab_x: f32) -> (Rect, f32) {
    let tab_w = text_w + USER_TAB_PAD_X * 2.0;
    let sel_y = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + (USER_SEL_H - USER_TAB_H) / 2.0;
    let rect = Rect::from_xywh(tab_x, sel_y, tab_w, USER_TAB_H);
    (rect, tab_x + tab_w + USER_TAB_GAP)
}

fn tab_rect(label: &str, cache: &RenderCache, tab_x: f32) -> (Rect, f32) {
    let text_w = if let Some(blob) = TextBlob::new(label, &cache.small_font) {
        blob.bounds().width()
    } else {
        label.len() as f32 * 7.0
    };
    tab_rect_with_width(text_w, tab_x)
}

fn tab_rect_approx(label: &str, tab_x: f32) -> (Rect, f32) {
    tab_rect_with_width(label.len() as f32 * 7.0, tab_x)
}

/// Rect and next-x for the "▾ Other" dropdown button.
fn dropdown_btn_rect(tab_x: f32) -> (Rect, f32) {
    let label = "Other \u{25be}";
    tab_rect_approx(label, tab_x)
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
    let sel_bg_y = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Separator bar under toolbar
    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, sel_bg_y, width, USER_SEL_H), &paint);

    let (_, font_metrics) = cache.small_font.metrics();
    let draw_pill =
        |canvas: &Canvas, label: &str, is_selected: bool, is_hovered: bool, tab_x: f32| -> f32 {
            let (rect, next_x) = tab_rect(label, cache, tab_x);
            let bg = if is_selected {
                0xff_4a90d9
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
            let mut p = Paint::default();
            p.set_anti_alias(true);
            p.set_color(Color::from(bg));
            p.set_style(PaintStyle::Fill);
            canvas.draw_rrect(rrect, &p);

            if let Some(blob) = TextBlob::new(label, &cache.small_font) {
                let bounds = blob.bounds();
                let tx = rect.left() + (rect.width() - bounds.width()) / 2.0 - bounds.left();
                let ty = rect.top()
                    + (rect.height() - (font_metrics.descent - font_metrics.ascent)) / 2.0
                    - font_metrics.ascent;
                p.set_color(Color::from(fg));
                p.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (tx, ty), &p);
            }
            next_x
        };

    let mut x = SIDE_PAD;

    // "Plan" pill
    x = draw_pill(
        canvas,
        "Plan",
        state.selected_user.is_none(),
        state.hovered_user_tab == Some(0),
        x,
    );

    // Current user pill (if signed in)
    let mut btn_idx = 1i32;
    if let Some(cur_id) = state.current_user
        && let Some(user) = plan.users_data.get(&cur_id).map(|ud| &ud.user)
    {
        x = draw_pill(
            canvas,
            &user.name,
            state.selected_user == Some(cur_id),
            state.hovered_user_tab == Some(btn_idx),
            x,
        );
        btn_idx += 1;
    }

    // "Other ▾" dropdown button if there are any other users
    let others = other_users(plan, state.current_user);
    if !others.is_empty() {
        let other_selected = state
            .selected_user
            .map(|id| state.current_user != Some(id))
            .unwrap_or(false);
        let label = if other_selected {
            // Show selected user name with chevron
            plan.users_data
                .get(&state.selected_user.unwrap())
                .map(|ud| format!("{} \u{25be}", ud.user.name))
                .unwrap_or_else(|| "Other \u{25be}".to_string())
        } else {
            "Other \u{25be}".to_string()
        };
        draw_pill(
            canvas,
            &label,
            other_selected,
            state.hovered_user_tab == Some(btn_idx),
            x,
        );

        // Draw dropdown if open
        if state.user_dropdown_open {
            draw_user_dropdown(canvas, state, cache, plan, &others, x, width);
        }
    }
}

fn draw_user_dropdown(
    canvas: &Canvas,
    state: &CalendarOverridesState,
    cache: &RenderCache,
    plan: &Plan,
    others: &[&plinko_shared::data::User],
    btn_x: f32,
    width: f32,
) {
    let drop_top = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + USER_SEL_H;
    let drop_w = 220.0f32;
    let drop_x = (btn_x - USER_TAB_GAP).min(width - drop_w - SIDE_PAD);

    let filter_lower = state.user_filter.to_lowercase();
    let filtered: Vec<_> = others
        .iter()
        .filter(|u| filter_lower.is_empty() || u.name.to_lowercase().contains(&filter_lower))
        .collect();

    let items_h = (filtered.len() as f32 * DROPDOWN_ITEM_H).min(DROPDOWN_MAX_H);
    let total_h = DROPDOWN_FILTER_H + items_h;

    let drop_rect = Rect::from_xywh(drop_x, drop_top, drop_w, total_h);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Background + border
    paint.set_color(Color::from(0xff_1e1e1e));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(RRect::new_rect_xy(drop_rect, 6.0, 6.0), &paint);
    paint.set_color(Color::from(0xff_444444));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(RRect::new_rect_xy(drop_rect, 6.0, 6.0), &paint);

    // Filter input
    let filter_rect = Rect::from_xywh(
        drop_x + 4.0,
        drop_top + 4.0,
        drop_w - 8.0,
        DROPDOWN_FILTER_H - 8.0,
    );
    paint.set_color(Color::from(0xff_2a2a2a));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(RRect::new_rect_xy(filter_rect, 4.0, 4.0), &paint);

    let display = if state.user_filter.is_empty() {
        "Filter...".to_string()
    } else {
        state.user_filter.clone()
    };
    let fg = if state.user_filter.is_empty() {
        0xff_666666u32
    } else {
        0xff_cccccc
    };
    if let Some(blob) = TextBlob::new(&display, &cache.small_font) {
        let (_, m) = cache.small_font.metrics();
        let tx = filter_rect.left() + 6.0;
        let ty =
            filter_rect.top() + (filter_rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(Color::from(fg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    // Clip items area
    let items_rect = Rect::from_xywh(drop_x, drop_top + DROPDOWN_FILTER_H, drop_w, items_h);
    canvas.save();
    canvas.clip_rect(items_rect, ClipOp::Intersect, true);

    for (i, user) in filtered.iter().enumerate() {
        let iy = drop_top + DROPDOWN_FILTER_H + i as f32 * DROPDOWN_ITEM_H;
        let item_rect = Rect::from_xywh(drop_x + 2.0, iy, drop_w - 4.0, DROPDOWN_ITEM_H);
        let is_selected = state.selected_user == Some(user.id);
        let is_hovered = state.hovered_dropdown_item == Some(i);

        if is_selected || is_hovered {
            let bg = if is_selected {
                0xff_4a90d9
            } else {
                0xff_2a2a2a
            };
            paint.set_color(Color::from(bg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(RRect::new_rect_xy(item_rect, 4.0, 4.0), &paint);
        }

        if let Some(blob) = TextBlob::new(&user.name, &cache.small_font) {
            let (_, m) = cache.small_font.metrics();
            let tx = item_rect.left() + 8.0;
            let ty =
                item_rect.top() + (item_rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
            let fg = if is_selected {
                0xff_ffffff
            } else {
                0xff_cccccc
            };
            paint.set_color(Color::from(fg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }

    canvas.restore();

    // Draw "no results" if filtered is empty
    if filtered.is_empty()
        && !plan.users_data.is_empty()
        && let Some(blob) = TextBlob::new("No match", &cache.small_font)
    {
        let (_, m) = cache.small_font.metrics();
        let tx = drop_x + 12.0;
        let ty = drop_top + DROPDOWN_FILTER_H + (DROPDOWN_ITEM_H - (m.descent - m.ascent)) / 2.0
            - m.ascent;
        paint.set_color(Color::from(0xff_666666));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
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
        .and_then(|uid| plan.user_calendar_overrides.get(uid));

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
/// Hit-test the quick-selector bar. Returns button index:
///   0 = Plan, 1 = current user (if present), last = "Other" dropdown btn.
pub fn hit_test_user_tab(
    x: f32,
    y: f32,
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
) -> Option<usize> {
    let sel_top = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + (USER_SEL_H - USER_TAB_H) / 2.0;
    let sel_bot = sel_top + USER_TAB_H;
    if y < sel_top || y > sel_bot {
        return None;
    }
    let mut tab_x = SIDE_PAD;
    let mut idx = 0usize;

    // "Plan" pill
    let (rect, next_x) = tab_rect_approx("Plan", tab_x);
    tab_x = next_x;
    if x >= rect.left() && x <= rect.right() {
        return Some(idx);
    }
    idx += 1;

    // Current user pill
    if let Some(cur_id) = current_user
        && let Some(user) = plan.users_data.get(&cur_id).map(|ud| &ud.user)
    {
        let (rect, next_x) = tab_rect_approx(&user.name, tab_x);
        tab_x = next_x;
        if x >= rect.left() && x <= rect.right() {
            return Some(idx);
        }
        idx += 1;
    }

    // "Other" dropdown button
    let others = other_users(plan, current_user);
    if !others.is_empty() {
        let label = "Other \u{25be}";
        let (rect, _) = tab_rect_approx(label, tab_x);
        if x >= rect.left() && x <= rect.right() {
            return Some(idx);
        }
    }

    None
}

/// Hit-test the dropdown list. Returns the index into the filtered user list.
pub fn hit_test_dropdown_item(
    x: f32,
    y: f32,
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
    filter: &str,
    btn_x_hint: f32,
    width: f32,
) -> Option<usize> {
    let drop_top = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + USER_SEL_H;
    let drop_w = 220.0f32;
    let drop_x = (btn_x_hint - USER_TAB_GAP).min(width - drop_w - SIDE_PAD);
    let filter_lower = filter.to_lowercase();
    let others = other_users(plan, current_user);
    let filtered: Vec<_> = others
        .iter()
        .filter(|u| filter_lower.is_empty() || u.name.to_lowercase().contains(&filter_lower))
        .collect();
    let items_h = (filtered.len() as f32 * DROPDOWN_ITEM_H).min(DROPDOWN_MAX_H);
    let items_top = drop_top + DROPDOWN_FILTER_H;

    if x < drop_x || x > drop_x + drop_w || y < items_top || y > items_top + items_h {
        return None;
    }
    let i = ((y - items_top) / DROPDOWN_ITEM_H) as usize;
    if i < filtered.len() {
        // Map back to original index in `others`
        let user_id = filtered[i].id;
        others.iter().position(|u| u.id == user_id)
    } else {
        None
    }
}

/// Returns the UserId for a given dropdown item index (filtered list).
pub fn user_for_dropdown_item(
    idx: usize,
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
    filter: &str,
) -> Option<plinko_shared::data::ids::UserId> {
    let filter_lower = filter.to_lowercase();
    let others = other_users(plan, current_user);
    others
        .iter()
        .filter(|u| filter_lower.is_empty() || u.name.to_lowercase().contains(&filter_lower))
        .nth(idx)
        .map(|u| u.id)
}

/// Returns the x coordinate at which the dropdown button starts (approx).
pub fn dropdown_btn_x(plan: &Plan, current_user: Option<plinko_shared::data::ids::UserId>) -> f32 {
    let mut x = SIDE_PAD;
    let (_, next_x) = tab_rect_approx("Plan", x);
    x = next_x;
    if let Some(cur_id) = current_user
        && let Some(user) = plan.users_data.get(&cur_id).map(|ud| &ud.user)
    {
        let (_, next_x) = tab_rect_approx(&user.name, x);
        x = next_x;
    }
    x
}

/// Returns true if (x, y) is inside the dropdown filter input box.
pub fn hit_test_dropdown_filter(
    x: f32,
    y: f32,
    plan: &Plan,
    current_user: Option<plinko_shared::data::ids::UserId>,
    width: f32,
) -> bool {
    let btn_x = dropdown_btn_x(plan, current_user);
    let drop_top = TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + USER_SEL_H;
    let drop_w = 220.0f32;
    let drop_x = (btn_x - USER_TAB_GAP).min(width - drop_w - SIDE_PAD);
    let filter_rect = Rect::from_xywh(
        drop_x + 4.0,
        drop_top + 4.0,
        drop_w - 8.0,
        DROPDOWN_FILTER_H - 8.0,
    );
    x >= filter_rect.left()
        && x <= filter_rect.right()
        && y >= filter_rect.top()
        && y <= filter_rect.bottom()
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
