//! Rendering functions for the allocation page.

use chrono::{Datelike, Duration, NaiveDate};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, Rect, TextBlob};

use crate::data::Plan;
use crate::data::ids::{TaskId, UserId};
use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;

use super::state::AllocationState;

const USER_LABEL_W: f32 = ALLOC_USER_LABEL_W;

fn header_top() -> f32 {
    TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0
}

fn rows_top() -> f32 {
    header_top() + GANTT_HEADER_H
}

fn date_to_x(date: NaiveDate, view_start: NaiveDate, zoom: f32, scroll_x: f32) -> f32 {
    let days = (date - view_start).num_days();
    days as f32 * zoom - scroll_x + USER_LABEL_W
}

/// Deterministic task color based on task index in sorted list.
fn task_color(idx: usize) -> u32 {
    TASK_COLORS[idx % TASK_COLORS.len()]
}

/// Returns the date range covered by the plan, or None if no tasks.
fn date_range(plan: &Plan) -> Option<(NaiveDate, NaiveDate)> {
    use crate::data::TaskAllocation;
    if !plan.node_allocations.has_schedule() {
        return None;
    }
    let start = plan
        .node_allocations
        .tasks
        .values()
        .map(|ts| ts.allocation.start_date())
        .min()
        .unwrap_or(plan.start_date)
        .min(plan.start_date);
    let end = plan
        .node_allocations
        .tasks
        .values()
        .map(|ts| ts.allocation.end_date())
        .max()
        .unwrap_or(plan.start_date);
    let _ = TaskAllocation::Dynamic {
        scheduled_start_date: start,
        scheduled_end_date: end,
        time_allocation: vec![],
    };
    // Extend slightly for padding
    Some((start - Duration::days(2), end + Duration::days(5)))
}

/// Compute (user_id, date) → vec of (task_index, hours) for all task allocations.
fn build_day_map(
    plan: &Plan,
    sorted_task_ids: &[TaskId],
) -> std::collections::HashMap<(UserId, NaiveDate), Vec<(usize, f32)>> {
    let mut map: std::collections::HashMap<(UserId, NaiveDate), Vec<(usize, f32)>> =
        std::collections::HashMap::new();

    for (task_idx, task_id) in sorted_task_ids.iter().enumerate() {
        if let Some(ts) = plan.node_allocations.tasks.get(task_id) {
            let time_allocation = match &ts.allocation {
                crate::data::TaskAllocation::Dynamic {
                    time_allocation, ..
                } => time_allocation,
                crate::data::TaskAllocation::Fixed {
                    time_allocation, ..
                } => time_allocation,
            };
            for seg in time_allocation {
                map.entry((seg.user, seg.date))
                    .or_default()
                    .push((task_idx, seg.hours_worked));
            }
        }
    }
    map
}

pub fn draw_allocation(
    canvas: &Canvas,
    width: f32,
    height: f32,
    state: &AllocationState,
    cache: &RenderCache,
    plan: &Plan,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Background
    paint.set_color(Color::from(GANTT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, header_top(), width, height - header_top()),
        &paint,
    );

    draw_toolbar_buttons(canvas, state, cache, width);

    // Sort users and tasks consistently
    let mut sorted_users: Vec<(&UserId, &crate::data::User)> = plan
        .users_data
        .iter()
        .map(|(id, ud)| (id, &ud.user))
        .collect();
    sorted_users.sort_by_key(|(_, u)| &u.name);

    let mut sorted_task_ids: Vec<TaskId> = plan.tasks.keys().copied().collect();
    sorted_task_ids.sort_by_key(|id| {
        plan.tasks
            .get(id)
            .map(|t| t.name.clone())
            .unwrap_or_default()
    });

    if !plan.node_allocations.has_schedule() {
        // No schedule computed — show message
        paint.set_color(Color::from(GANTT_HEADER_FG));
        paint.set_style(PaintStyle::Fill);
        if let Some(blob) = TextBlob::new("No schedule computed yet.", &cache.font) {
            let bounds = blob.bounds();
            let x = (width - bounds.width()) / 2.0 - bounds.left();
            let y = (height - bounds.height()) / 2.0 - bounds.top();
            canvas.draw_text_blob(&blob, (x, y), &paint);
        }
        return;
    }

    let (view_start, view_end) =
        date_range(plan).unwrap_or((plan.start_date, plan.start_date + Duration::days(30)));

    let day_map = build_day_map(plan, &sorted_task_ids);

    let num_rows = sorted_users.len();
    let total_content_h = num_rows as f32 * GANTT_ROW_H;
    let visible_h = (height - rows_top()).max(0.0);
    let center_offset = ((visible_h - total_content_h) / 2.0).max(0.0);

    // Clip content area
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top(), width, height - rows_top()),
        ClipOp::Intersect,
        false,
    );

    // Row backgrounds
    for (row_idx, _) in sorted_users.iter().enumerate() {
        let row_y = rows_top() + center_offset + row_idx as f32 * GANTT_ROW_H - state.scroll_y;
        let bg = if row_idx % 2 == 0 {
            GANTT_BG
        } else {
            ALLOC_ROW_ALT_BG
        };
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(Rect::from_xywh(0.0, row_y, width, GANTT_ROW_H), &paint);
    }

    // Day columns and bars
    let mut date = view_start;
    while date <= view_end {
        let x = date_to_x(date, view_start, state.zoom, state.scroll_x);
        if x < USER_LABEL_W - state.zoom {
            date += Duration::days(1);
            continue;
        }
        if x > width {
            break;
        }

        // Weekend / weekday column background
        let weekday = date.weekday();
        let is_weekend = matches!(weekday, chrono::Weekday::Sat | chrono::Weekday::Sun);
        if is_weekend {
            paint.set_color(Color::from(GANTT_WEEKEND_BG));
            paint.set_style(PaintStyle::Fill);
            for row_idx in 0..sorted_users.len() {
                let row_y =
                    rows_top() + center_offset + row_idx as f32 * GANTT_ROW_H - state.scroll_y;
                canvas.draw_rect(Rect::from_xywh(x, row_y, state.zoom, GANTT_ROW_H), &paint);
            }
        }

        // Day separator
        paint.set_color(Color::from(GANTT_DAY_LINE_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line(
            (x, rows_top()),
            (x, rows_top() + total_content_h.max(height - rows_top())),
            &paint,
        );

        // Stacked bars for each user on this day
        for (row_idx, (user_id, _)) in sorted_users.iter().enumerate() {
            let row_y = rows_top() + center_offset + row_idx as f32 * GANTT_ROW_H - state.scroll_y;
            let capacity = plan.hours_available(user_id, date);
            if capacity <= 0.0 {
                date += Duration::days(1);
                continue;
            }

            if let Some(segments) = day_map.get(&(**user_id, date)) {
                let bar_w = (state.zoom - 2.0).max(1.0);
                let mut stacked_y = row_y + GANTT_ROW_H - GANTT_ROW_PADDING;

                for &(task_idx, hours) in segments {
                    let frac = (hours / capacity).clamp(0.0, 1.0);
                    let bar_h = frac * (GANTT_ROW_H - 2.0 * GANTT_ROW_PADDING);
                    if bar_h < 0.5 {
                        continue;
                    }
                    stacked_y -= bar_h;
                    let color = task_color(task_idx);
                    paint.set_color(Color::from(color));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_rect(Rect::from_xywh(x + 1.0, stacked_y, bar_w, bar_h), &paint);
                }
            }
        }

        date += Duration::days(1);
    }

    canvas.restore();

    // User name labels (clipped to label column)
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top(), USER_LABEL_W, height - rows_top()),
        ClipOp::Intersect,
        false,
    );
    for (row_idx, (_, user)) in sorted_users.iter().enumerate() {
        let row_y = rows_top() + center_offset + row_idx as f32 * GANTT_ROW_H - state.scroll_y;
        // Label background
        let bg = if row_idx % 2 == 0 {
            GANTT_BG
        } else {
            ALLOC_ROW_ALT_BG
        };
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(0.0, row_y, USER_LABEL_W - 8.0, GANTT_ROW_H),
            &paint,
        );

        if let Some(blob) = TextBlob::new(&user.name, &cache.small_font) {
            let (_, metrics) = cache.small_font.metrics();
            let text_h = metrics.descent - metrics.ascent;
            let ty = row_y + (GANTT_ROW_H - text_h) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(GANTT_HEADER_FG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_text_blob(&blob, (8.0, ty), &paint);
        }
    }
    canvas.restore();

    // Separator line between label column and bars
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line((USER_LABEL_W, rows_top()), (USER_LABEL_W, height), &paint);

    draw_header(canvas, state, width, view_start, cache);
}

fn draw_header(
    canvas: &Canvas,
    state: &AllocationState,
    width: f32,
    view_start: NaiveDate,
    cache: &RenderCache,
) {
    let top = header_top();
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Header background
    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(USER_LABEL_W, top, width - USER_LABEL_W, GANTT_HEADER_H),
        &paint,
    );

    // Month row (top part of header)
    let mut cur = view_start;
    let end_date = view_start + Duration::days(((width / state.zoom) as i64 + 60).max(60));
    while cur <= end_date {
        let x = date_to_x(cur, view_start, state.zoom, state.scroll_x);
        if x > width {
            break;
        }
        // Draw month label at start of each month
        if cur == view_start || cur.day() == 1 {
            let label = cur.format("%b %Y").to_string();
            if let Some(blob) = TextBlob::new(&label, &cache.small_font) {
                paint.set_color(Color::from(GANTT_HEADER_MONTH_FG));
                paint.set_style(PaintStyle::Fill);
                let (_, m) = cache.small_font.metrics();
                canvas.draw_text_blob(
                    &blob,
                    (
                        x.max(USER_LABEL_W + 4.0),
                        top + GANTT_MONTH_ROW_H / 2.0 - m.ascent / 2.0,
                    ),
                    &paint,
                );
            }
        }
        // Advance to next month start or end
        let next_month = if cur.month() == 12 {
            NaiveDate::from_ymd_opt(cur.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(cur.year(), cur.month() + 1, 1).unwrap()
        };
        cur = next_month;
    }

    // Day row (bottom part of header) — draw day numbers
    let day_top = top + GANTT_MONTH_ROW_H;
    let min_day_w = 20.0;
    let show_days = state.zoom >= min_day_w;

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(USER_LABEL_W, day_top, width - USER_LABEL_W, GANTT_DAY_ROW_H),
        ClipOp::Intersect,
        false,
    );

    let days_visible = ((width / state.zoom) as i64 + 4).max(4);
    let mut d = view_start;
    for _ in 0..days_visible {
        let x = date_to_x(d, view_start, state.zoom, state.scroll_x);
        if x > width {
            break;
        }
        if x >= USER_LABEL_W && show_days {
            let label = d.day().to_string();
            if let Some(blob) = TextBlob::new(&label, &cache.small_font) {
                let bounds = blob.bounds();
                let lx = x + (state.zoom - bounds.width()) / 2.0 - bounds.left();
                let (_, m) = cache.small_font.metrics();
                let ly = day_top + (GANTT_DAY_ROW_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
                let wd = d.weekday();
                let is_we = matches!(wd, chrono::Weekday::Sat | chrono::Weekday::Sun);
                paint.set_color(Color::from(if is_we {
                    0xff_aaaaaa
                } else {
                    GANTT_HEADER_FG
                }));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (lx, ly), &paint);
            }
        }
        d += Duration::days(1);
    }
    canvas.restore();

    // Header bottom border
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line(
        (USER_LABEL_W, top + GANTT_HEADER_H),
        (width, top + GANTT_HEADER_H),
        &paint,
    );
}

pub fn draw_toolbar_buttons(
    canvas: &Canvas,
    state: &AllocationState,
    cache: &RenderCache,
    width: f32,
) {
    // Today button (0)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(0),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(0),
        &cache.icon_today,
    );
    // Person button (1, right side)
    icon_button::draw_icon_button(
        canvas,
        person_right_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(1),
        &cache.icon_person,
    );
    // Settings button (2, rightmost)
    icon_button::draw_icon_button(
        canvas,
        settings_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(2),
        &cache.icon_settings,
    );
}

pub fn hit_test_toolbar_buttons(x: f32, y: f32, width: f32) -> Option<usize> {
    let btn_y = TOOLBAR_BTN_Y;
    let s = TOOLBAR_BTN_SIZE;
    let btns = [
        (toolbar_btn_x(0), btn_y),
        (person_right_btn_x(width), btn_y),
        (settings_btn_x(width), btn_y),
    ];
    for (i, (bx, by)) in btns.iter().enumerate() {
        if x >= *bx && x <= *bx + s && y >= *by && y <= *by + s {
            return Some(i);
        }
    }
    None
}
