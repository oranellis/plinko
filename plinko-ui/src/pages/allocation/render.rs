//! Rendering for the allocation page.
//!
//! Layout:
//!   Left: user selector panel (ALLOC_USER_PANEL_W wide)
//!   Right: per-selected-user timeline
//!     - Date header (month row + day row)
//!     - Utilisation row: per-day bar, colour-coded green/amber/red
//!     - One task row per task that has work for this user

use chrono::{Datelike, Duration, NaiveDate};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, RRect, Rect, TextBlob};

use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;
use plinko_shared::data::ids::{TaskId, UserId};
use plinko_shared::data::task::WorkerSlot;
use plinko_shared::data::{Plan, TaskAllocation};

use super::state::AllocationState;

// ── Layout helpers ────────────────────────────────────────────────────────── {{{

fn content_top() -> f32 {
    TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0
}

fn header_top() -> f32 {
    content_top()
}

fn timeline_top() -> f32 {
    content_top() + GANTT_HEADER_H + ALLOC_UTIL_ROW_H
}

fn util_row_top() -> f32 {
    content_top() + GANTT_HEADER_H
}

fn date_to_x(date: NaiveDate, view_start: NaiveDate, zoom: f32, scroll_x: f32) -> f32 {
    let days = (date - view_start).num_days();
    days as f32 * zoom - scroll_x + ALLOC_USER_PANEL_W + ALLOC_TASK_LABEL_W
}

/// Left edge of the timeline area (after user panel + task label column).
fn timeline_left() -> f32 {
    ALLOC_USER_PANEL_W + ALLOC_TASK_LABEL_W
}

fn task_color(idx: usize) -> u32 {
    TASK_COLORS[idx % TASK_COLORS.len()]
}

fn date_range(plan: &Plan) -> Option<(NaiveDate, NaiveDate)> {
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
    Some((start - Duration::days(2), end + Duration::days(5)))
}

/// Returns tasks that have at least one WorkSegment for `user_id`, sorted by task name.
fn tasks_for_user<'a>(plan: &'a Plan, user_id: &UserId) -> Vec<(&'a TaskId, String)> {
    let mut result: Vec<(&TaskId, String)> = plan
        .node_allocations
        .tasks
        .iter()
        .filter(|(_, ts)| {
            let segs = match &ts.allocation {
                TaskAllocation::Dynamic {
                    time_allocation, ..
                }
                | TaskAllocation::Fixed {
                    time_allocation, ..
                } => time_allocation,
            };
            segs.iter().any(|s| &s.user == user_id)
        })
        .map(|(id, _)| {
            let name = plan
                .tasks
                .get(id)
                .map(|t| t.name.clone())
                .unwrap_or_default();
            (id, name)
        })
        .collect();
    result.sort_by(|(_, a), (_, b)| a.cmp(b));
    result
}

/// Returns sorted list of all users (by name) with their average daily utilisation [0..1].
pub fn sorted_users_with_util_pub(plan: &Plan) -> Vec<(&UserId, &plinko_shared::data::User, f32)> {
    sorted_users_with_util(plan)
}

fn sorted_users_with_util(plan: &Plan) -> Vec<(&UserId, &plinko_shared::data::User, f32)> {
    let mut users: Vec<_> = plan
        .users_data
        .iter()
        .map(|(id, ud)| {
            let util = average_utilisation(plan, id);
            (id, &ud.user, util)
        })
        .collect();
    users.sort_by_key(|(_, u, _)| u.name.clone());
    users
}

/// Average daily utilisation for a user across all scheduled days with capacity > 0.
fn average_utilisation(plan: &Plan, user_id: &UserId) -> f32 {
    let (start, end) = match date_range(plan) {
        Some(r) => r,
        None => return 0.0,
    };
    let mut total_cap = 0.0_f32;
    let mut total_used = 0.0_f32;
    let mut d = start;
    while d <= end {
        let cap = plan.hours_available(user_id, d);
        if cap > 0.0 {
            total_cap += cap;
            // Sum work segments for this user on this day
            for ts in plan.node_allocations.tasks.values() {
                let segs = match &ts.allocation {
                    TaskAllocation::Dynamic {
                        time_allocation, ..
                    }
                    | TaskAllocation::Fixed {
                        time_allocation, ..
                    } => time_allocation,
                };
                for seg in segs {
                    if &seg.user == user_id && seg.date == d {
                        total_used += seg.hours_worked;
                    }
                }
            }
        }
        d += Duration::days(1);
    }
    if total_cap > 0.0 {
        total_used / total_cap
    } else {
        0.0
    }
}

fn util_color(frac: f32) -> u32 {
    if frac >= 1.0 {
        ALLOC_UTIL_RED
    } else if frac >= 0.8 {
        ALLOC_UTIL_AMBER
    } else {
        ALLOC_UTIL_GREEN
    }
}

// }}}

// ── Public entry point ────────────────────────────────────────────────────── {{{

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

    // Full background
    paint.set_color(Color::from(GANTT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, content_top(), width, height - content_top()),
        &paint,
    );

    draw_toolbar_buttons(canvas, state, cache, width);

    let sorted_users = sorted_users_with_util(plan);

    if !plan.node_allocations.has_schedule() {
        paint.set_color(Color::from(GANTT_HEADER_FG));
        if let Some(blob) = TextBlob::new("No schedule computed yet.", &cache.font) {
            let bounds = blob.bounds();
            let x = (width - bounds.width()) / 2.0 - bounds.left();
            let y = (height - bounds.height()) / 2.0 - bounds.top();
            canvas.draw_text_blob(&blob, (x, y), &paint);
        }
        draw_user_panel(canvas, height, state, cache, plan, &sorted_users);
        draw_user_panel_border(canvas, height);
        return;
    }

    let (view_start, view_end) =
        date_range(plan).unwrap_or((plan.start_date, plan.start_date + Duration::days(30)));

    if let Some(uid) = &state.selected_user {
        draw_date_header(canvas, state, width, view_start, cache);
        draw_util_row(
            canvas, state, plan, width, view_start, view_end, uid, &mut paint,
        );
        draw_task_rows(
            canvas, state, plan, width, height, view_start, view_end, uid, cache, &mut paint,
        );
    } else {
        // Prompt to select a user
        paint.set_color(Color::from(GANTT_HEADER_FG));
        if let Some(blob) = TextBlob::new("Select a user to view their allocation.", &cache.font) {
            let bounds = blob.bounds();
            let x =
                timeline_left() + (width - timeline_left() - bounds.width()) / 2.0 - bounds.left();
            let y = (height - bounds.height()) / 2.0 - bounds.top();
            canvas.draw_text_blob(&blob, (x, y), &paint);
        }
    }

    draw_user_panel(canvas, height, state, cache, plan, &sorted_users);
    draw_user_panel_border(canvas, height);
}

// }}}

// ── User panel ────────────────────────────────────────────────────────────── {{{

fn draw_user_panel(
    canvas: &Canvas,
    height: f32,
    state: &AllocationState,
    cache: &RenderCache,
    _plan: &Plan,
    sorted_users: &[(&UserId, &plinko_shared::data::User, f32)],
) {
    let top = content_top();
    let panel_h = height - top;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Panel background
    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, top, ALLOC_USER_PANEL_W, panel_h),
        &paint,
    );

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, top, ALLOC_USER_PANEL_W, panel_h),
        ClipOp::Intersect,
        false,
    );

    for (idx, (uid, user, util)) in sorted_users.iter().enumerate() {
        let entry_y = top + idx as f32 * ALLOC_USER_ENTRY_H - state.user_panel_scroll;
        if entry_y + ALLOC_USER_ENTRY_H < top || entry_y > height {
            continue;
        }

        // Selection / hover background
        let bg = if state.selected_user.as_ref() == Some(uid) {
            ALLOC_SELECTED_BG
        } else if state.hovered_user.as_ref() == Some(uid) {
            ALLOC_HOVER_BG
        } else if idx % 2 == 1 {
            ALLOC_ROW_ALT_BG
        } else {
            0xff_ffffff
        };
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(0.0, entry_y, ALLOC_USER_PANEL_W, ALLOC_USER_ENTRY_H),
            &paint,
        );

        // Name
        let (_, metrics) = cache.small_font.metrics();
        let ascent = metrics.ascent;
        let descent = metrics.descent;
        let text_h = descent - ascent;

        let name_y = entry_y + 10.0 - ascent;
        paint.set_color(Color::from(0xff_222222));
        paint.set_style(PaintStyle::Fill);
        // Clip name to panel width - padding
        canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(10.0, entry_y, ALLOC_USER_PANEL_W - 20.0, ALLOC_USER_ENTRY_H),
            ClipOp::Intersect,
            false,
        );
        if let Some(blob) = TextBlob::new(&user.name, &cache.small_font) {
            canvas.draw_text_blob(&blob, (10.0, name_y), &paint);
        }
        canvas.restore();

        // Mini util bar
        let bar_x = 10.0;
        let bar_y = entry_y + 10.0 + text_h + 6.0;
        let bar_w = ALLOC_USER_PANEL_W - 20.0;
        let bar_h = 6.0;

        // Bar track
        paint.set_color(Color::from(0xff_e0e0e0));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(Rect::from_xywh(bar_x, bar_y, bar_w, bar_h), &paint);

        // Fill
        let fill_w = (util * bar_w).min(bar_w);
        if fill_w > 0.0 {
            paint.set_color(Color::from(util_color(*util)));
            canvas.draw_rect(Rect::from_xywh(bar_x, bar_y, fill_w, bar_h), &paint);
        }

        // Percent label
        let pct = format!("{:.0}%", util * 100.0);
        if let Some(blob) = TextBlob::new(&pct, &cache.small_font) {
            paint.set_color(Color::from(0xff_666666));
            let lx = bar_x + bar_w + 4.0; // put it just after bar
            let ly = bar_y - ascent;
            // Only draw if there's room
            if lx + blob.bounds().width() < ALLOC_USER_PANEL_W {
                canvas.draw_text_blob(&blob, (lx, ly), &paint);
            }
        }

        // Bottom separator
        paint.set_color(Color::from(0xff_e8e8e8));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line(
            (0.0, entry_y + ALLOC_USER_ENTRY_H - 0.5),
            (ALLOC_USER_PANEL_W, entry_y + ALLOC_USER_ENTRY_H - 0.5),
            &paint,
        );
    }

    canvas.restore();
}

fn draw_user_panel_border(canvas: &Canvas, height: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line(
        (ALLOC_USER_PANEL_W, content_top()),
        (ALLOC_USER_PANEL_W, height),
        &paint,
    );
}

// }}}

// ── Date header ───────────────────────────────────────────────────────────── {{{

fn draw_date_header(
    canvas: &Canvas,
    state: &AllocationState,
    width: f32,
    view_start: NaiveDate,
    cache: &RenderCache,
) {
    let top = header_top();
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(
            timeline_left(),
            top,
            width - timeline_left(),
            GANTT_HEADER_H,
        ),
        &paint,
    );

    // Month row
    let mut cur = view_start;
    let end_date = view_start + Duration::days(((width / state.zoom) as i64 + 60).max(60));
    while cur <= end_date {
        let x = date_to_x(cur, view_start, state.zoom, state.scroll_x);
        if x > width {
            break;
        }
        if cur == view_start || cur.day() == 1 {
            let label = cur.format("%b %Y").to_string();
            if let Some(blob) = TextBlob::new(&label, &cache.small_font) {
                let (_, m) = cache.small_font.metrics();
                paint.set_color(Color::from(GANTT_HEADER_MONTH_FG));
                paint.set_style(PaintStyle::Fill);
                canvas.save();
                canvas.clip_rect(
                    Rect::from_xywh(
                        timeline_left(),
                        top,
                        width - timeline_left(),
                        GANTT_MONTH_ROW_H,
                    ),
                    ClipOp::Intersect,
                    false,
                );
                canvas.draw_text_blob(
                    &blob,
                    (
                        x.max(timeline_left() + 4.0),
                        top + GANTT_MONTH_ROW_H / 2.0 - m.ascent / 2.0,
                    ),
                    &paint,
                );
                canvas.restore();
            }
        }
        let next_month = if cur.month() == 12 {
            NaiveDate::from_ymd_opt(cur.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(cur.year(), cur.month() + 1, 1).unwrap()
        };
        cur = next_month;
    }

    // Day row
    let day_top = top + GANTT_MONTH_ROW_H;
    let show_days = state.zoom >= 20.0;
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            timeline_left(),
            day_top,
            width - timeline_left(),
            GANTT_DAY_ROW_H,
        ),
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
        if x >= timeline_left() && show_days {
            let label = d.day().to_string();
            if let Some(blob) = TextBlob::new(&label, &cache.small_font) {
                let bounds = blob.bounds();
                let lx = x + (state.zoom - bounds.width()) / 2.0 - bounds.left();
                let (_, m) = cache.small_font.metrics();
                let ly = day_top + (GANTT_DAY_ROW_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
                let is_we = matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun);
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

    // Bottom border
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line(
        (timeline_left(), top + GANTT_HEADER_H),
        (width, top + GANTT_HEADER_H),
        &paint,
    );
}

// }}}

// ── Utilisation row ───────────────────────────────────────────────────────── {{{

#[allow(clippy::too_many_arguments)]
fn draw_util_row(
    canvas: &Canvas,
    state: &AllocationState,
    plan: &Plan,
    width: f32,
    view_start: NaiveDate,
    view_end: NaiveDate,
    user_id: &UserId,
    paint: &mut Paint,
) {
    let top = util_row_top();
    let bottom = top + ALLOC_UTIL_ROW_H;

    // Background
    paint.set_color(Color::from(0xff_f8f8f8));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(
            timeline_left(),
            top,
            width - timeline_left(),
            ALLOC_UTIL_ROW_H,
        ),
        paint,
    );

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            timeline_left(),
            top,
            width - timeline_left(),
            ALLOC_UTIL_ROW_H,
        ),
        ClipOp::Intersect,
        false,
    );

    let mut d = view_start;
    while d <= view_end {
        let x = date_to_x(d, view_start, state.zoom, state.scroll_x);
        if x > width {
            break;
        }
        if x + state.zoom < timeline_left() {
            d += Duration::days(1);
            continue;
        }
        let cap = plan.hours_available(user_id, d);
        if cap > 0.0 {
            // Total hours worked on this day
            let used: f32 = plan
                .node_allocations
                .tasks
                .values()
                .flat_map(|ts| {
                    let segs = match &ts.allocation {
                        TaskAllocation::Dynamic {
                            time_allocation, ..
                        }
                        | TaskAllocation::Fixed {
                            time_allocation, ..
                        } => time_allocation,
                    };
                    segs.iter()
                })
                .filter(|s| &s.user == user_id && s.date == d)
                .map(|s| s.hours_worked)
                .sum();

            let frac = used / cap;
            if frac > 0.0 {
                let bar_h = ((frac).min(1.0) * (ALLOC_UTIL_ROW_H - 4.0)).max(2.0);
                let bar_y = bottom - 2.0 - bar_h;
                let bar_w = (state.zoom - 2.0).max(1.0);
                paint.set_color(Color::from(util_color(frac)));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rect(Rect::from_xywh(x + 1.0, bar_y, bar_w, bar_h), paint);

                // Overflow cap indicator
                if frac > 1.0 {
                    let overflow_h = ((frac - 1.0) * (ALLOC_UTIL_ROW_H - 4.0)).min(4.0);
                    paint.set_color(Color::from(0xff_cc0000));
                    canvas.draw_rect(
                        Rect::from_xywh(x + 1.0, bar_y - overflow_h, bar_w, overflow_h),
                        paint,
                    );
                }
            }
        }
        d += Duration::days(1);
    }

    canvas.restore();

    // Bottom border
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line((timeline_left(), bottom), (width, bottom), paint);
}

// }}}

// ── Task rows ─────────────────────────────────────────────────────────────── {{{

#[allow(clippy::too_many_arguments)]
fn draw_task_rows(
    canvas: &Canvas,
    state: &AllocationState,
    plan: &Plan,
    width: f32,
    height: f32,
    view_start: NaiveDate,
    view_end: NaiveDate,
    user_id: &UserId,
    cache: &RenderCache,
    paint: &mut Paint,
) {
    let top = timeline_top();
    let content_h = height - top;

    let user_tasks = tasks_for_user(plan, user_id);

    // Sorted task ids for stable color mapping
    let mut all_task_ids: Vec<TaskId> = plan.tasks.keys().copied().collect();
    all_task_ids.sort_by_key(|id| {
        plan.tasks
            .get(id)
            .map(|t| t.name.clone())
            .unwrap_or_default()
    });
    let color_index = |tid: &TaskId| all_task_ids.iter().position(|t| t == tid).unwrap_or(0);

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(timeline_left(), top, width - timeline_left(), content_h),
        ClipOp::Intersect,
        false,
    );

    // Column backgrounds (weekend tint + day lines)
    let num_rows = user_tasks.len();
    let rows_h = (num_rows as f32 * GANTT_ROW_H).max(content_h);
    let mut d = view_start;
    while d <= view_end {
        let x = date_to_x(d, view_start, state.zoom, state.scroll_x);
        if x > width {
            break;
        }
        if x + state.zoom < timeline_left() {
            d += Duration::days(1);
            continue;
        }
        let is_we = matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun);
        if is_we {
            paint.set_color(Color::from(GANTT_WEEKEND_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(Rect::from_xywh(x, top, state.zoom, rows_h), paint);
        }
        paint.set_color(Color::from(GANTT_DAY_LINE_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line((x, top), (x, top + rows_h), paint);
        d += Duration::days(1);
    }

    // Row backgrounds and task bars
    for (row_idx, (task_id, _task_name)) in user_tasks.iter().enumerate() {
        let row_y = top + row_idx as f32 * GANTT_ROW_H;
        let bg = if row_idx % 2 == 1 {
            ALLOC_ROW_ALT_BG
        } else {
            GANTT_BG
        };
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(timeline_left(), row_y, width - timeline_left(), GANTT_ROW_H),
            paint,
        );

        let cidx = color_index(task_id);
        let base_color = task_color(cidx);

        if let Some(ts) = plan.node_allocations.tasks.get(task_id) {
            let segs = match &ts.allocation {
                TaskAllocation::Dynamic {
                    time_allocation, ..
                }
                | TaskAllocation::Fixed {
                    time_allocation, ..
                } => time_allocation,
            };
            for seg in segs {
                if &seg.user != user_id {
                    continue;
                }
                let x = date_to_x(seg.date, view_start, state.zoom, state.scroll_x);
                if x + state.zoom < timeline_left() || x > width {
                    continue;
                }

                let cap = plan.hours_available(user_id, seg.date);
                let frac = if cap > 0.0 {
                    (seg.hours_worked / cap).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let bar_h = (frac * (GANTT_ROW_H - 2.0 * GANTT_ROW_PADDING)).max(2.0);
                let bar_y = row_y + GANTT_ROW_H - GANTT_ROW_PADDING - bar_h;
                let bar_w = (state.zoom - 2.0).max(1.0);

                paint.set_color(Color::from(base_color));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rect(Rect::from_xywh(x + 1.0, bar_y, bar_w, bar_h), paint);
            }

            // (Task names are drawn in the fixed label column below)
        }

        // Row bottom separator
        paint.set_color(Color::from(GANTT_HEADER_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line(
            (timeline_left(), row_y + GANTT_ROW_H),
            (width, row_y + GANTT_ROW_H),
            paint,
        );
    }

    // Today line
    {
        use chrono::Local;
        let today = Local::now().date_naive();
        let tx = date_to_x(today, view_start, state.zoom, state.scroll_x);
        if tx >= timeline_left() && tx <= width {
            paint.set_color(Color::from(0xcc_4a90d9));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(2.0);
            canvas.draw_line((tx, top), (tx, top + rows_h), paint);
        }
    }

    canvas.restore();

    // ── Fixed task-name label column ──────────────────────────────────────
    {
        let label_left = ALLOC_USER_PANEL_W;
        let label_w = ALLOC_TASK_LABEL_W;

        // Background
        paint.set_color(Color::from(GANTT_HEADER_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(Rect::from_xywh(label_left, top, label_w, content_h), paint);

        canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(label_left, top, label_w, content_h),
            ClipOp::Intersect,
            false,
        );

        let (_, m) = cache.small_font.metrics();
        for (row_idx, (_task_id, task_name)) in user_tasks.iter().enumerate() {
            let row_y = top + row_idx as f32 * GANTT_ROW_H;
            let bg = if row_idx % 2 == 1 {
                ALLOC_ROW_ALT_BG
            } else {
                GANTT_HEADER_BG
            };
            paint.set_color(Color::from(bg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(
                Rect::from_xywh(label_left, row_y, label_w, GANTT_ROW_H),
                paint,
            );

            // Label text, clipped to column width
            if let Some(blob) = TextBlob::new(task_name, &cache.small_font) {
                let ly = row_y + (GANTT_ROW_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
                canvas.save();
                canvas.clip_rect(
                    Rect::from_xywh(label_left + 6.0, row_y, label_w - 12.0, GANTT_ROW_H),
                    ClipOp::Intersect,
                    false,
                );
                paint.set_color(Color::from(0xff_222222));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (label_left + 6.0, ly), paint);
                canvas.restore();
            }

            // Row separator
            paint.set_color(Color::from(GANTT_HEADER_BORDER));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.0);
            canvas.draw_line(
                (label_left, row_y + GANTT_ROW_H),
                (label_left + label_w, row_y + GANTT_ROW_H),
                paint,
            );
        }
        canvas.restore();

        // Right border of label column
        paint.set_color(Color::from(GANTT_HEADER_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line(
            (label_left + label_w, top),
            (label_left + label_w, height),
            paint,
        );
    }

    // Hover info panel
    if let Some(hovered) = state.hovered_task_idx
        && hovered < user_tasks.len()
    {
        let (task_id, task_name) = &user_tasks[hovered];
        draw_hover_info(
            canvas, state, plan, width, height, task_id, task_name, user_id, cache,
        );
    }
}

// }}}

// ── Hover info panel ─────────────────────────────────────────────────────── {{{

#[allow(clippy::too_many_arguments)]
fn draw_hover_info(
    canvas: &Canvas,
    _state: &AllocationState,
    plan: &Plan,
    _width: f32,
    height: f32,
    task_id: &TaskId,
    task_name: &str,
    user_id: &UserId,
    cache: &RenderCache,
) {
    let task = match plan.tasks.get(task_id) {
        Some(t) => t,
        None => return,
    };
    let ts = match plan.node_allocations.tasks.get(task_id) {
        Some(t) => t,
        None => return,
    };

    let segs: Vec<_> = match &ts.allocation {
        TaskAllocation::Dynamic {
            time_allocation, ..
        }
        | TaskAllocation::Fixed {
            time_allocation, ..
        } => time_allocation,
    }
    .iter()
    .filter(|s| &s.user == user_id)
    .collect();

    // Collect all unique allocated users across the whole task (all segments).
    let all_segs = match &ts.allocation {
        TaskAllocation::Dynamic {
            time_allocation, ..
        }
        | TaskAllocation::Fixed {
            time_allocation, ..
        } => time_allocation,
    };
    let mut allocated_user_ids: Vec<UserId> = Vec::new();
    for s in all_segs {
        if !allocated_user_ids.contains(&s.user) {
            allocated_user_ids.push(s.user);
        }
    }
    // For Specific slots whose user has no segments yet, include them too.
    for slot in &task.workers {
        if let WorkerSlot::Specific { user_id: uid, .. } = slot
            && !allocated_user_ids.contains(uid)
        {
            allocated_user_ids.push(*uid);
        }
    }

    let total_h: f32 = segs.iter().map(|s| s.hours_worked).sum();
    let first_day = segs.iter().map(|s| s.date).min();
    let last_day = segs.iter().map(|s| s.date).max();

    let mut lines: Vec<String> = vec![task_name.to_string()];

    // Workers line: names of allocated users.
    if !allocated_user_ids.is_empty() {
        let worker_names: Vec<&str> = allocated_user_ids
            .iter()
            .filter_map(|uid| plan.user(uid).map(|u| u.name.as_str()))
            .collect();
        if !worker_names.is_empty() {
            lines.push(format!("Workers: {}", worker_names.join(", ")));
        }
    }

    if let (Some(s), Some(e)) = (first_day, last_day) {
        lines.push(format!("{} – {}", s.format("%d %b"), e.format("%d %b %Y")));
    }
    lines.push(format!("{:.1}h allocated", total_h));
    if !task.description.is_empty() {
        let desc = if task.description.len() > 60 {
            format!("{}…", &task.description[..60])
        } else {
            task.description.clone()
        };
        lines.push(desc);
    }

    if lines.is_empty() {
        return;
    }

    let (_, metrics) = cache.small_font.metrics();
    let line_h = (metrics.descent - metrics.ascent).ceil();
    let line_gap = 3.0_f32;
    let pad = 10.0_f32;
    let margin = 8.0_f32;

    let max_w = lines
        .iter()
        .map(|l| cache.small_font.measure_str(l.as_str(), None).0)
        .fold(0.0_f32, f32::max);

    let panel_w = max_w + pad * 2.0;
    let panel_h = lines.len() as f32 * line_h + (lines.len() - 1) as f32 * line_gap + pad * 2.0;
    let px = timeline_left() + margin;
    let py = height - margin - panel_h;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(0x30_000000_u32));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(px + 2.0, py + 3.0, panel_w, panel_h),
            6.0,
            6.0,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(0xf4_ffffff_u32));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_xywh(px, py, panel_w, panel_h), 6.0, 6.0),
        &paint,
    );

    // Border
    paint.set_color(Color::from(INPUT_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_xywh(px, py, panel_w, panel_h), 6.0, 6.0),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    for (i, line) in lines.iter().enumerate() {
        if let Some(blob) = TextBlob::new(line.as_str(), &cache.small_font) {
            let color = if i == 0 { INPUT_FG } else { MUTED_FG };
            paint.set_color(Color::from(color));
            let ty = py + pad + i as f32 * (line_h + line_gap) - metrics.ascent;
            canvas.draw_text_blob(&blob, (px + pad, ty), &paint);
        }
    }
}

// }}}

// ── Toolbar ───────────────────────────────────────────────────────────────── {{{

pub fn draw_toolbar_buttons(
    canvas: &Canvas,
    state: &AllocationState,
    cache: &RenderCache,
    width: f32,
) {
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(0),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(0),
        &cache.icon_today,
    );
    icon_button::draw_icon_button(
        canvas,
        person_right_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(1),
        &cache.icon_person,
    );
    icon_button::draw_icon_button(
        canvas,
        settings_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(2),
        &cache.icon_settings,
    );
}

pub fn hit_test_toolbar_buttons(x: f32, y: f32, width: f32) -> Option<usize> {
    let s = TOOLBAR_BTN_SIZE;
    let btns = [
        (toolbar_btn_x(0), TOOLBAR_BTN_Y),
        (person_right_btn_x(width), TOOLBAR_BTN_Y),
        (settings_btn_x(width), TOOLBAR_BTN_Y),
    ];
    for (i, (bx, by)) in btns.iter().enumerate() {
        if x >= *bx && x <= *bx + s && y >= *by && y <= *by + s {
            return Some(i);
        }
    }
    None
}

// }}}

// ── Hit testing ──────────────────────────────────────────────────────────── {{{

/// Returns the UserId at (x, y) in the user panel, or None.
pub fn hit_test_user_panel<'a>(
    x: f32,
    y: f32,
    height: f32,
    state: &AllocationState,
    sorted_users: &[(&'a UserId, &'a plinko_shared::data::User, f32)],
) -> Option<&'a UserId> {
    if !(0.0..=ALLOC_USER_PANEL_W).contains(&x) {
        return None;
    }
    let top = content_top();
    if !(top..=height).contains(&y) {
        return None;
    }
    for (idx, (uid, _, _)) in sorted_users.iter().enumerate() {
        let entry_y = top + idx as f32 * ALLOC_USER_ENTRY_H - state.user_panel_scroll;
        if y >= entry_y && y < entry_y + ALLOC_USER_ENTRY_H {
            return Some(uid);
        }
    }
    None
}

/// Returns the task row index hovered in the timeline, or None.
pub fn hit_test_task_row(x: f32, y: f32, plan: &Plan, user_id: &UserId) -> Option<usize> {
    if x <= timeline_left() {
        return None;
    }
    let top = timeline_top();
    if y < top {
        return None;
    }
    let tasks = tasks_for_user(plan, user_id);
    let row = ((y - top) / GANTT_ROW_H) as usize;
    if row < tasks.len() { Some(row) } else { None }
}

// }}}
