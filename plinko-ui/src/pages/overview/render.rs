//! Rendering functions for the overview page Gantt chart.

use chrono::{Datelike, Duration, NaiveDate, Weekday as CWeekday};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, PathBuilder, RRect, Rect, TextBlob};
use std::collections::{HashMap, HashSet};

use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;
use plinko_shared::data::Plan;
use plinko_shared::data::Status;
use plinko_shared::data::ids::{MilestoneId, NodeId, TaskId, UserId};

use super::gantt::{
    GanttItem, GanttRow, MilestoneStatus, compute_date_range, milestone_display_status, pack_rows,
};
use super::state::OverviewState;

/// Size of the warning triangle icon (both width and height).
const WARN_SIZE: f32 = 14.0;

/// A clicked item on the Gantt chart.
pub enum GanttHit {
    Task(TaskId),
    Milestone(MilestoneId),
}

// ── Layout helpers ─────────────────────────────────────────────────────────────

fn gantt_header_top() -> f32 {
    TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0
}

fn gantt_rows_top() -> f32 {
    gantt_header_top() + GANTT_HEADER_H
}

pub fn gantt_header_h() -> f32 {
    GANTT_HEADER_H
}

fn date_to_x(date: NaiveDate, view_start: NaiveDate, zoom: f32, scroll_x: f32) -> f32 {
    let days = (date - view_start).num_days();
    days as f32 * zoom - scroll_x
}

pub fn view_start_date(plan: &Plan) -> NaiveDate {
    compute_date_range(plan)
        .map(|(s, _)| s)
        .unwrap_or(plan.start_date)
}

/// Vertical offset to center the content rows in the visible Gantt area.
/// Returns 0 when content exceeds the available height (scrollable).
fn vertical_center_offset(num_rows: usize, height: f32) -> f32 {
    let content_h = num_rows as f32 * GANTT_ROW_H;
    let visible_h = (height - gantt_rows_top()).max(0.0);
    ((visible_h - content_h) / 2.0).max(0.0)
}

/// Compute the y pixel of a Gantt row's top edge (accounting for centering).
fn row_top_y(row_idx: usize, num_rows: usize, height: f32, scroll_y: f32) -> f32 {
    gantt_rows_top() + vertical_center_offset(num_rows, height) + row_idx as f32 * GANTT_ROW_H
        - scroll_y
}

// ── Main entry point ───────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn draw_overview(
    canvas: &Canvas,
    _x: f32,
    _y: f32,
    w: f32,
    h: f32,
    state: &OverviewState,
    cache: &RenderCache,
    plan: &Plan,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(GANTT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, gantt_header_top(), w, h - gantt_header_top()),
        &paint,
    );

    let rows = pack_rows(plan);
    let view_start = view_start_date(plan);

    // Draw in layered order: backgrounds → grid lines → dependency arrows → items.
    draw_gantt_row_backgrounds(canvas, state, &rows, w, h);
    draw_gantt_grid(canvas, state, w, h, view_start);
    draw_gantt_dependencies(canvas, state, plan, &rows, w, h, view_start);
    if state.hovered_node.is_some() {
        draw_highlighted_dep_arrows(canvas, state, plan, &rows, w, h, view_start);
    }
    draw_gantt_rows(canvas, state, plan, &rows, w, h, view_start, cache);
    draw_gantt_header(canvas, state, w, view_start, cache);
    draw_toolbar_buttons(canvas, state, cache, w);

    // Draw warning tooltip on top of everything else.
    if let Some(node_id) = state.hovered_warning
        && let Some(violation) = plan.node_allocations.constraint_violations.get(&node_id)
    {
        draw_warning_tooltip(
            canvas,
            violation,
            state.cursor_x,
            state.cursor_y,
            w,
            h,
            cache,
        );
    }

    // Draw info panel in bottom-left when hovering a gantt item.
    if let Some(node_id) = state.hovered_node {
        draw_node_info_panel(canvas, node_id, plan, w, h, cache);
    }
}

// ── Toolbar buttons ────────────────────────────────────────────────────────────

fn draw_toolbar_buttons(canvas: &Canvas, state: &OverviewState, cache: &RenderCache, width: f32) {
    // Left-side buttons: today (0), add-task (1), add-milestone (2)
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(0),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(0),
        &cache.icon_today,
    );
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(1),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(1),
        &cache.icon_plus,
    );
    icon_button::draw_icon_button(
        canvas,
        toolbar_btn_x(2),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(2),
        &cache.icon_diamond,
    );
    // Right-side buttons: person (3), settings (4)
    icon_button::draw_icon_button(
        canvas,
        person_right_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(3),
        &cache.icon_person,
    );
    icon_button::draw_icon_button(
        canvas,
        settings_btn_x(width),
        TOOLBAR_BTN_Y,
        state.toolbar_btn_hovered == Some(4),
        &cache.icon_settings,
    );
}

pub fn hit_test_toolbar_buttons(px: f32, py: f32, width: f32) -> Option<usize> {
    for i in 0..3_u32 {
        if icon_button::hit_test_icon_button(px, py, toolbar_btn_x(i), TOOLBAR_BTN_Y) {
            return Some(i as usize);
        }
    }
    if icon_button::hit_test_icon_button(px, py, person_right_btn_x(width), TOOLBAR_BTN_Y) {
        return Some(3);
    }
    if icon_button::hit_test_icon_button(px, py, settings_btn_x(width), TOOLBAR_BTN_Y) {
        return Some(4);
    }
    None
}

// ── Gantt header ───────────────────────────────────────────────────────────────

fn draw_gantt_header(
    canvas: &Canvas,
    state: &OverviewState,
    width: f32,
    view_start: NaiveDate,
    cache: &RenderCache,
) {
    let header_top = gantt_header_top();
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;
    let days_visible = (width / zoom).ceil() as i64 + 4;
    let first_offset = (scroll_x / zoom).floor() as i64 - 1;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Header background
    paint.set_color(Color::from(GANTT_HEADER_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, header_top, width, GANTT_HEADER_H),
        &paint,
    );

    let month_row_top = header_top;
    let day_row_top = header_top + GANTT_MONTH_ROW_H;
    let (_, metrics) = cache.font.metrics();

    // Build month segments. For each month, x_start is computed from the ACTUAL
    // first day of that month (not the first visible day) so panning is smooth.
    // x_end is updated each iteration to extend the segment to the right.
    let mut month_segments: Vec<(String, f32, f32)> = Vec::new(); // (label, x_start, x_end)
    let mut last_month: Option<(u32, i32)> = None;

    for day_offset in first_offset..=first_offset + days_visible {
        let date = view_start + Duration::days(day_offset);
        let x = day_offset as f32 * zoom - scroll_x;
        if x > width + zoom {
            break;
        }
        let (y, m) = (date.year(), date.month());

        // Month segments — x_start is always the true first-of-month position
        match last_month {
            Some((pm, py)) if pm == m && py == y => {
                if let Some(last) = month_segments.last_mut() {
                    last.2 = x + zoom;
                }
            }
            _ => {
                last_month = Some((m, y));
                let label = format!("{} {}", month_abbr(m), y);
                // Compute x for the actual first day of this month (may be off-screen left)
                let first_of_month = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(date);
                let fom_offset = (first_of_month - view_start).num_days();
                let x_month_start = fom_offset as f32 * zoom - scroll_x;
                month_segments.push((label, x_month_start, x + zoom));
            }
        }

        // Day number (no separator lines in header)
        if zoom >= 16.0 {
            let day_label = format!("{}", date.day());
            let tw = cache.font.measure_str(&day_label, None).0;
            let day_x = x + zoom / 2.0 - tw / 2.0;
            // Baseline near bottom of day row
            let day_y = day_row_top + GANTT_DAY_ROW_H - metrics.descent - 3.0;
            paint.set_color(Color::from(GANTT_HEADER_FG));
            paint.set_style(PaintStyle::Fill);
            if let Some(blob) = TextBlob::new(&day_label, &cache.font) {
                canvas.draw_text_blob(&blob, (day_x, day_y), &paint);
            }
        }
    }

    // Month labels — centred in visible portion of each segment (no separators)
    for (label, x_start, x_end) in &month_segments {
        let vis_start = x_start.max(0.0);
        let vis_end = x_end.min(width);
        if vis_end <= vis_start {
            continue;
        }
        let tw = cache.font.measure_str(label, None).0;
        let seg_center = (vis_start + vis_end) / 2.0;
        let label_x = (seg_center - tw / 2.0).max(vis_start + 4.0);
        // Vertically centred in the month row — baseline
        let label_y =
            month_row_top + GANTT_MONTH_ROW_H / 2.0 - (metrics.ascent + metrics.descent) / 2.0;
        paint.set_color(Color::from(GANTT_HEADER_MONTH_FG));
        paint.set_style(PaintStyle::Fill);
        if let Some(blob) = TextBlob::new(label, &cache.font) {
            canvas.draw_text_blob(&blob, (label_x, label_y), &paint);
        }
    }

    // Bottom border of header (only)
    let border_y = header_top + GANTT_HEADER_H - 0.5;
    paint.set_color(Color::from(GANTT_HEADER_BORDER));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line((0.0, border_y), (width, border_y), &paint);
    paint.set_style(PaintStyle::Fill);
}

fn month_abbr(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

// ── Alternating row backgrounds ───────────────────────────────────────────────

/// Draw alternating row stripes beneath the grid and task bars.
/// Stripes fill the entire visible Gantt area (not just content rows) and are
/// positioned using the vertical center offset so the pattern is consistent
/// with where content rows are rendered.
/// Must be called before [`draw_gantt_grid`] so day-separator lines render on top.
fn draw_gantt_row_backgrounds(
    canvas: &Canvas,
    state: &OverviewState,
    rows: &[GanttRow],
    width: f32,
    height: f32,
) {
    let rows_top = gantt_rows_top();
    let scroll_y = state.scroll_y;
    let center_off = vertical_center_offset(rows.len(), height);
    let visible_h = height - rows_top;

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top, width, visible_h),
        ClipOp::Intersect,
        false,
    );

    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_style(PaintStyle::Fill);
    paint.set_color(Color::from(GANTT_ROW_ALT_BG));

    // Compute which row-index slots are visible (may be negative if scrolled).
    // row0_rel is the y-position of row 0's top edge relative to rows_top.
    let row0_rel = center_off - scroll_y;
    let first = ((-row0_rel) / GANTT_ROW_H).floor() as i64;
    let last = ((visible_h - row0_rel) / GANTT_ROW_H).ceil() as i64;

    for idx in first..=last {
        if idx.rem_euclid(2) == 1 {
            let row_y = rows_top + row0_rel + idx as f32 * GANTT_ROW_H;
            canvas.draw_rect(Rect::from_xywh(0.0, row_y, width, GANTT_ROW_H), &paint);
        }
    }

    canvas.restore();
}

// ── Day grid ───────────────────────────────────────────────────────────────────

fn draw_gantt_grid(
    canvas: &Canvas,
    state: &OverviewState,
    width: f32,
    height: f32,
    view_start: NaiveDate,
) {
    let rows_top = gantt_rows_top();
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;
    let days_visible = (width / zoom).ceil() as i64 + 2;
    let first_offset = (scroll_x / zoom).floor() as i64 - 1;
    let today = chrono::Local::now().date_naive();

    let mut paint = Paint::default();
    paint.set_anti_alias(false);

    for day_offset in first_offset..=first_offset + days_visible + 2 {
        let date = view_start + Duration::days(day_offset);
        let x = day_offset as f32 * zoom - scroll_x;
        if x + zoom < 0.0 {
            continue;
        }
        if x > width {
            break;
        }

        // Weekend shading (fill the day column)
        let wd = date.weekday();
        if wd == CWeekday::Sat || wd == CWeekday::Sun {
            paint.set_color(Color::from(GANTT_WEEKEND_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(
                Rect::from_xywh(x, rows_top, zoom, height - rows_top),
                &paint,
            );
        }

        // Vertical day separator line
        paint.set_style(PaintStyle::Stroke);
        if date == today {
            paint.set_color(Color::from(GANTT_TODAY_LINE_COLOR));
            paint.set_stroke_width(2.0);
        } else {
            paint.set_color(Color::from(GANTT_DAY_LINE_COLOR));
            paint.set_stroke_width(GANTT_DAY_LINE_W);
        }
        canvas.draw_line((x, rows_top), (x, height), &paint);
    }
}

// ── Task bars and milestone diamonds ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_gantt_rows(
    canvas: &Canvas,
    state: &OverviewState,
    plan: &Plan,
    rows: &[GanttRow],
    width: f32,
    height: f32,
    view_start: NaiveDate,
    cache: &RenderCache,
) {
    let rows_top = gantt_rows_top();
    let scroll_y = state.scroll_y;
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;

    // Nodes with no dependents — drawn with an end-node flag.
    let dependents_map = plan.build_dependents_map();
    let end_nodes: HashSet<NodeId> = plan
        .tasks
        .keys()
        .map(|id| NodeId::Task(*id))
        .chain(plan.milestones.keys().map(|id| NodeId::Milestone(*id)))
        .filter(|nid| dependents_map.get(nid).is_none_or(|v| v.is_empty()))
        .collect();

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top, width, height - rows_top),
        ClipOp::Intersect,
        false,
    );

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let (_, metrics) = cache.font.metrics();

    for (row_idx, row) in rows.iter().enumerate() {
        let row_y = row_top_y(row_idx, rows.len(), height, scroll_y);

        for item in &row.items {
            match item {
                GanttItem::Task { id, start, end } => {
                    let task = match plan.tasks.get(id) {
                        Some(t) => t,
                        None => continue,
                    };
                    let bar_x = date_to_x(*start, view_start, zoom, scroll_x);
                    let bar_w = (((*end - *start).num_days() + 1) as f32 * zoom).max(4.0);
                    let bar_y = row_y + GANTT_ROW_PADDING;
                    let bar_h = GANTT_ROW_H - 2.0 * GANTT_ROW_PADDING;
                    let node_id = NodeId::Task(*id);

                    let is_hovered = state.hovered_node == Some(node_id);
                    let is_upstream = !is_hovered && state.hovered_deps.contains(&node_id);
                    let is_downstream =
                        !is_hovered && !is_upstream && state.hovered_dependents.contains(&node_id);
                    if is_hovered || is_upstream || is_downstream {
                        let (glow_color, border_color, glow_w, border_w) = if is_hovered {
                            (HOVER_SELF_GLOW, HOVER_SELF_BORDER, 7.0_f32, 1.75_f32)
                        } else if is_upstream {
                            (
                                HOVER_UPSTREAM_GLOW,
                                HOVER_UPSTREAM_BORDER,
                                4.5_f32,
                                1.25_f32,
                            )
                        } else {
                            (
                                HOVER_DOWNSTREAM_GLOW,
                                HOVER_DOWNSTREAM_BORDER,
                                4.5_f32,
                                1.25_f32,
                            )
                        };
                        paint.set_style(PaintStyle::Stroke);
                        paint.set_stroke_width(glow_w);
                        paint.set_color(Color::from(glow_color));
                        canvas.draw_rrect(
                            RRect::new_rect_xy(
                                Rect::from_xywh(bar_x, bar_y, bar_w, bar_h),
                                GANTT_BAR_CORNER,
                                GANTT_BAR_CORNER,
                            ),
                            &paint,
                        );
                        paint.set_stroke_width(border_w);
                        paint.set_color(Color::from(border_color));
                        canvas.draw_rrect(
                            RRect::new_rect_xy(
                                Rect::from_xywh(bar_x, bar_y, bar_w, bar_h),
                                GANTT_BAR_CORNER,
                                GANTT_BAR_CORNER,
                            ),
                            &paint,
                        );
                        paint.set_style(PaintStyle::Fill);
                    }

                    let bar_color = task_status_color(plan.task_status(id));
                    paint.set_color(Color::from(bar_color));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_rrect(
                        RRect::new_rect_xy(
                            Rect::from_xywh(bar_x, bar_y, bar_w, bar_h),
                            GANTT_BAR_CORNER,
                            GANTT_BAR_CORNER,
                        ),
                        &paint,
                    );

                    // Plan-target border — drawn on top of the fill.
                    if plan.scheduler_target == node_id {
                        paint.set_style(PaintStyle::Stroke);
                        paint.set_stroke_width(6.0);
                        paint.set_color(Color::from(TARGET_GLOW));
                        canvas.draw_rrect(
                            RRect::new_rect_xy(
                                Rect::from_xywh(bar_x, bar_y, bar_w, bar_h),
                                GANTT_BAR_CORNER,
                                GANTT_BAR_CORNER,
                            ),
                            &paint,
                        );
                        paint.set_stroke_width(1.5);
                        paint.set_color(Color::from(TARGET_BORDER));
                        canvas.draw_rrect(
                            RRect::new_rect_xy(
                                Rect::from_xywh(bar_x, bar_y, bar_w, bar_h),
                                GANTT_BAR_CORNER,
                                GANTT_BAR_CORNER,
                            ),
                            &paint,
                        );
                        paint.set_style(PaintStyle::Fill);
                    }

                    if bar_w > 12.0 {
                        canvas.save();
                        canvas.clip_rect(
                            Rect::from_xywh(bar_x + 4.0, bar_y, bar_w - 8.0, bar_h),
                            ClipOp::Intersect,
                            false,
                        );
                        let label_color = label_color_for(bar_color);
                        paint.set_color(Color::from(label_color));
                        if let Some(blob) = TextBlob::new(&task.name, &cache.font) {
                            let text_y = bar_y + (bar_h - (metrics.descent - metrics.ascent)) / 2.0
                                - metrics.ascent;
                            canvas.draw_text_blob(&blob, (bar_x + 6.0, text_y), &paint);
                        }
                        canvas.restore();
                    }

                    // Warning icon if this task has a constraint violation.
                    let has_violation = plan
                        .node_allocations
                        .constraint_violations
                        .contains_key(&node_id);
                    if has_violation {
                        let warn_rect = warn_icon_rect_for_task(bar_x, bar_y, bar_w, bar_h);
                        let hovered = state.hovered_warning == Some(node_id);
                        draw_warning_icon(canvas, warn_rect, hovered, &mut paint);
                    }

                    // End-node flag if nothing depends on this task.
                    if end_nodes.contains(&node_id) {
                        let flag_x = bar_x + bar_w;
                        let mid_y = bar_y + bar_h / 2.0;
                        draw_end_node_flag(canvas, flag_x, mid_y, bar_h, &mut paint);
                    }
                }

                GanttItem::Milestone { id, date } => {
                    let node_id = NodeId::Milestone(*id);
                    let ms_status = milestone_display_status(plan, *id);
                    let ms_color = milestone_color(ms_status);
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    let cy = row_y + GANTT_ROW_H / 2.0;
                    let half = GANTT_MS_HALF;

                    let mut pb = PathBuilder::new();
                    pb.move_to((cx, cy - half));
                    pb.line_to((cx + half, cy));
                    pb.line_to((cx, cy + half));
                    pb.line_to((cx - half, cy));
                    pb.close();
                    let ms_path = pb.detach();

                    let is_hovered = state.hovered_node == Some(node_id);
                    let is_upstream = !is_hovered && state.hovered_deps.contains(&node_id);
                    let is_downstream =
                        !is_hovered && !is_upstream && state.hovered_dependents.contains(&node_id);
                    if is_hovered || is_upstream || is_downstream {
                        let (glow_color, border_color, glow_w, border_w) = if is_hovered {
                            (HOVER_SELF_GLOW, HOVER_SELF_BORDER, 7.0_f32, 1.75_f32)
                        } else if is_upstream {
                            (
                                HOVER_UPSTREAM_GLOW,
                                HOVER_UPSTREAM_BORDER,
                                4.5_f32,
                                1.25_f32,
                            )
                        } else {
                            (
                                HOVER_DOWNSTREAM_GLOW,
                                HOVER_DOWNSTREAM_BORDER,
                                4.5_f32,
                                1.25_f32,
                            )
                        };
                        paint.set_style(PaintStyle::Stroke);
                        paint.set_stroke_width(glow_w);
                        paint.set_color(Color::from(glow_color));
                        canvas.draw_path(&ms_path, &paint);
                        paint.set_stroke_width(border_w);
                        paint.set_color(Color::from(border_color));
                        canvas.draw_path(&ms_path, &paint);
                        paint.set_style(PaintStyle::Fill);
                    }

                    paint.set_color(Color::from(ms_color));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_path(&ms_path, &paint);

                    paint.set_color(Color::from(darken(ms_color)));
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&ms_path, &paint);
                    paint.set_style(PaintStyle::Fill);

                    // Plan-target border — drawn on top of the fill.
                    if plan.scheduler_target == node_id {
                        paint.set_style(PaintStyle::Stroke);
                        paint.set_stroke_width(6.0);
                        paint.set_color(Color::from(TARGET_GLOW));
                        canvas.draw_path(&ms_path, &paint);
                        paint.set_stroke_width(1.5);
                        paint.set_color(Color::from(TARGET_BORDER));
                        canvas.draw_path(&ms_path, &paint);
                        paint.set_style(PaintStyle::Fill);
                    }

                    if let Some(ms) = plan.milestones.get(id) {
                        draw_milestone_label(
                            canvas,
                            rows,
                            row_idx,
                            cx,
                            cy,
                            half,
                            &ms.name,
                            GANTT_HEADER_FG,
                            &cache.font,
                            &metrics,
                            view_start,
                            zoom,
                            scroll_x,
                        );
                    }

                    // Warning icon if this milestone has a constraint violation.
                    let has_violation = plan
                        .node_allocations
                        .constraint_violations
                        .contains_key(&node_id);
                    if has_violation {
                        let warn_rect = warn_icon_rect_for_milestone(cx, cy);
                        let hovered = state.hovered_warning == Some(node_id);
                        draw_warning_icon(canvas, warn_rect, hovered, &mut paint);
                    }

                    // End-node flag if nothing depends on this milestone.
                    if end_nodes.contains(&node_id) {
                        draw_end_node_flag(canvas, cx + half, cy, half * 2.0, &mut paint);
                    }
                }

                GanttItem::PlanStart { date } => {
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    let cy = row_y + GANTT_ROW_H / 2.0;
                    let half = GANTT_MS_HALF * 1.1; // slightly larger for emphasis

                    let mut pb = PathBuilder::new();
                    pb.move_to((cx, cy - half));
                    pb.line_to((cx + half, cy));
                    pb.line_to((cx, cy + half));
                    pb.line_to((cx - half, cy));
                    pb.close();
                    let ps_path = pb.detach();

                    paint.set_color(Color::from(GANTT_PLAN_START_COLOR));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_path(&ps_path, &paint);

                    paint.set_color(Color::from(darken(GANTT_PLAN_START_COLOR)));
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&ps_path, &paint);
                    paint.set_style(PaintStyle::Fill);

                    draw_milestone_label(
                        canvas,
                        rows,
                        row_idx,
                        cx,
                        cy,
                        half,
                        "Plan Start",
                        GANTT_PLAN_START_COLOR,
                        &cache.font,
                        &metrics,
                        view_start,
                        zoom,
                        scroll_x,
                    );
                }
            }
        }
    }

    canvas.restore();
}

// ── Warning icon helpers ───────────────────────────────────────────────────────

/// Returns the rect for a task's warning icon (to the right of the bar, row-centered).
fn warn_icon_rect_for_task(bar_x: f32, bar_y: f32, bar_w: f32, bar_h: f32) -> Rect {
    let wx = bar_x + bar_w + 3.0;
    let wy = bar_y + (bar_h - WARN_SIZE) / 2.0;
    Rect::from_xywh(wx, wy, WARN_SIZE, WARN_SIZE)
}

/// Returns the rect for a milestone's warning icon (to the right of the diamond, row-centered).
fn warn_icon_rect_for_milestone(cx: f32, cy: f32) -> Rect {
    let wx = cx + GANTT_MS_HALF + 3.0;
    let wy = cy - WARN_SIZE / 2.0;
    Rect::from_xywh(wx, wy, WARN_SIZE, WARN_SIZE)
}

/// Draw a warning triangle icon inside `rect`.
fn draw_warning_icon(canvas: &Canvas, rect: Rect, hovered: bool, paint: &mut Paint) {
    let cx = rect.center_x();
    let top = rect.top();
    let bot = rect.bottom();
    let left = rect.left();
    let right = rect.right();

    // Slightly enlarge on hover.
    let (cx, top, bot, left, right) = if hovered {
        let expand = 2.0;
        (
            cx,
            top - expand,
            bot + expand,
            left - expand,
            right + expand,
        )
    } else {
        (cx, top, bot, left, right)
    };

    let mut pb = PathBuilder::new();
    pb.move_to((cx, top));
    pb.line_to((right, bot));
    pb.line_to((left, bot));
    pb.close();
    let tri = pb.detach();

    paint.set_style(PaintStyle::Fill);
    paint.set_color(Color::from(WARN_FILL));
    canvas.draw_path(&tri, paint);

    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    paint.set_color(Color::from(WARN_STROKE));
    canvas.draw_path(&tri, paint);
    paint.set_style(PaintStyle::Fill);

    // Draw "!" as two small filled rects (line + dot).
    paint.set_color(Color::from(WARN_ICON_GLYPH)); // dark glyph on amber triangle
    let bang_h = (bot - top) * 0.45;
    let bang_w = 2.0_f32;
    let bang_top = top + (bot - top) * 0.22;
    canvas.draw_rect(
        Rect::from_xywh(cx - bang_w / 2.0, bang_top, bang_w, bang_h),
        paint,
    );
    canvas.draw_circle((cx, bot - (bot - top) * 0.15), bang_w / 2.0 + 0.5, paint);
}

/// Draw a tooltip near `(cursor_x, cursor_y)` showing the constraint violation details.
fn draw_warning_tooltip(
    canvas: &Canvas,
    violation: &plinko_shared::data::ConstraintViolation,
    cursor_x: f32,
    cursor_y: f32,
    width: f32,
    height: f32,
    cache: &RenderCache,
) {
    let line1 = format!("Constraint violation: {}", violation.node_name);
    let kind_str = match violation.kind {
        plinko_shared::data::ConstraintKind::Fixed => "Fixed",
        plinko_shared::data::ConstraintKind::Latest => "Latest",
        plinko_shared::data::ConstraintKind::Earliest => "Earliest",
    };
    let line2 = format!(
        "{} required: {}  |  Scheduled: {}",
        kind_str, violation.required_date, violation.scheduled_date
    );

    let pad = 8.0_f32;
    let line_gap = 4.0_f32;

    let blob1 = TextBlob::new(&line1, &cache.small_font);
    let blob2 = TextBlob::new(&line2, &cache.small_font);

    let (_, metrics) = cache.small_font.metrics();
    let line_h = (metrics.descent - metrics.ascent).ceil();

    let tip_w = blob1
        .as_ref()
        .map(|b| b.bounds().width())
        .unwrap_or(0.0)
        .max(blob2.as_ref().map(|b| b.bounds().width()).unwrap_or(0.0))
        + pad * 2.0;
    let tip_h = line_h * 2.0 + line_gap + pad * 2.0;

    // Position tooltip above and to the right of cursor, clamped to screen.
    let mut tx = cursor_x + 12.0;
    let mut ty = cursor_y - tip_h - 8.0;
    if tx + tip_w > width - 4.0 {
        tx = width - tip_w - 4.0;
    }
    if ty < gantt_rows_top() {
        ty = cursor_y + 18.0;
    }
    if ty + tip_h > height {
        ty = height - tip_h - 4.0;
    }

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Background
    paint.set_color(Color::from(WARN_TOOLTIP_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_xywh(tx, ty, tip_w, tip_h), 4.0, 4.0),
        &paint,
    );

    // Text
    paint.set_color(Color::from(WARN_TOOLTIP_FG));
    let text_x = tx + pad;
    let y1 = ty + pad - metrics.ascent;
    let y2 = y1 + line_h + line_gap;
    if let Some(b) = blob1 {
        canvas.draw_text_blob(&b, (text_x, y1), &paint);
    }
    if let Some(b) = blob2 {
        canvas.draw_text_blob(&b, (text_x, y2), &paint);
    }
}

/// Returns the `NodeId` of a warning icon hit at `(x, y)`, if any.
pub fn hit_test_warning_icon(
    x: f32,
    y: f32,
    plan: &Plan,
    rows: &[GanttRow],
    state: &OverviewState,
    height: f32,
    view_start: NaiveDate,
) -> Option<NodeId> {
    if plan.node_allocations.constraint_violations.is_empty() {
        return None;
    }

    let zoom = state.zoom;
    let scroll_x = state.scroll_x;
    let scroll_y = state.scroll_y;

    for (row_idx, row) in rows.iter().enumerate() {
        let row_y = row_top_y(row_idx, rows.len(), height, scroll_y);

        for item in &row.items {
            match item {
                GanttItem::Task { id, start, end } => {
                    let node_id = NodeId::Task(*id);
                    if !plan
                        .node_allocations
                        .constraint_violations
                        .contains_key(&node_id)
                    {
                        continue;
                    }
                    let bar_x = date_to_x(*start, view_start, zoom, scroll_x);
                    let bar_w = (((*end - *start).num_days() + 1) as f32 * zoom).max(4.0);
                    let bar_y = row_y + GANTT_ROW_PADDING;
                    let bar_h = GANTT_ROW_H - 2.0 * GANTT_ROW_PADDING;
                    let warn_rect = warn_icon_rect_for_task(bar_x, bar_y, bar_w, bar_h);
                    if x >= warn_rect.left()
                        && x <= warn_rect.right()
                        && y >= warn_rect.top()
                        && y <= warn_rect.bottom()
                    {
                        return Some(node_id);
                    }
                }
                GanttItem::Milestone { id, date } => {
                    let node_id = NodeId::Milestone(*id);
                    if !plan
                        .node_allocations
                        .constraint_violations
                        .contains_key(&node_id)
                    {
                        continue;
                    }
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    let cy = row_y + GANTT_ROW_H / 2.0;
                    let warn_rect = warn_icon_rect_for_milestone(cx, cy);
                    if x >= warn_rect.left()
                        && x <= warn_rect.right()
                        && y >= warn_rect.top()
                        && y <= warn_rect.bottom()
                    {
                        return Some(node_id);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn task_status_color(status: Status) -> u32 {
    match status {
        Status::NotStarted => GANTT_TASK_NOT_STARTED,
        Status::InProgress => GANTT_TASK_IN_PROGRESS,
        Status::OnHold => GANTT_TASK_ON_HOLD,
        Status::Complete => GANTT_TASK_COMPLETE,
        Status::Dropped => GANTT_TASK_DROPPED,
    }
}

fn milestone_color(status: MilestoneStatus) -> u32 {
    match status {
        MilestoneStatus::NotStarted => GANTT_MS_NOT_STARTED,
        MilestoneStatus::InProgress => GANTT_MS_IN_PROGRESS,
        MilestoneStatus::Complete => GANTT_MS_COMPLETE,
    }
}

fn label_color_for(bg: u32) -> u32 {
    let r = ((bg >> 16) & 0xff) as f32;
    let g = ((bg >> 8) & 0xff) as f32;
    let b = (bg & 0xff) as f32;
    if 0.299 * r + 0.587 * g + 0.114 * b > 160.0 {
        GANTT_TASK_LABEL_DARK
    } else {
        GANTT_TASK_LABEL_LIGHT
    }
}

fn darken(c: u32) -> u32 {
    let a = c >> 24;
    let r = (((c >> 16) & 0xff) as f32 * 0.7) as u32;
    let g = (((c >> 8) & 0xff) as f32 * 0.7) as u32;
    let b = ((c & 0xff) as f32 * 0.7) as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

/// Draws a small flag (vertical staff + right-pointing triangle) to mark end nodes.
///
/// The flag is drawn at `(x, mid_y)` where `x` is just to the right of the bar/diamond
/// and `mid_y` is the vertical centre of the row.  `height` is the full available height
/// of the staff (equal to the bar height or `GANTT_MS_HALF * 2`).
fn draw_end_node_flag(canvas: &Canvas, x: f32, mid_y: f32, height: f32, paint: &mut Paint) {
    let staff_h = height;
    let flag_w = (height * 0.45).max(6.0);
    let flag_h = (height * 0.45).max(6.0);
    let gap = 3.0_f32;

    let top_y = mid_y - staff_h / 2.0;
    let bot_y = mid_y + staff_h / 2.0;
    let staff_x = x + gap;

    paint.set_color(Color::from(GANTT_END_NODE_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    canvas.draw_line((staff_x, top_y), (staff_x, bot_y), paint);

    let mut pb = PathBuilder::new();
    pb.move_to((staff_x, top_y));
    pb.line_to((staff_x + flag_w, top_y + flag_h / 2.0));
    pb.line_to((staff_x, top_y + flag_h));
    pb.close();
    let flag_path = pb.detach();

    paint.set_style(PaintStyle::Fill);
    canvas.draw_path(&flag_path, paint);
    paint.set_style(PaintStyle::Fill); // reset
}

// ── Milestone label helpers ────────────────────────────────────────────────────

/// Returns the visual pixel x-range `(left, right)` of a Gantt item.
fn item_pixel_range(
    item: &GanttItem,
    view_start: NaiveDate,
    zoom: f32,
    scroll_x: f32,
) -> (f32, f32) {
    match item {
        GanttItem::Task { start, end, .. } => {
            let x1 = date_to_x(*start, view_start, zoom, scroll_x);
            let x2 = date_to_x(*end, view_start, zoom, scroll_x) + zoom;
            (x1, x2)
        }
        GanttItem::Milestone { date, .. } | GanttItem::PlanStart { date } => {
            let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
            (cx - GANTT_MS_HALF * 1.1, cx + GANTT_MS_HALF * 1.1)
        }
    }
}

/// Returns `true` if any item in `rows[row_idx]` has a visual x-range that
/// overlaps `[px_start, px_end)`.
fn row_has_item_in_range(
    rows: &[GanttRow],
    row_idx: usize,
    px_start: f32,
    px_end: f32,
    view_start: NaiveDate,
    zoom: f32,
    scroll_x: f32,
) -> bool {
    rows.get(row_idx).is_some_and(|row| {
        row.items.iter().any(|item| {
            let (is, ie) = item_pixel_range(item, view_start, zoom, scroll_x);
            ie > px_start && is < px_end
        })
    })
}

/// Draw a milestone label, choosing the first clear side in the order
/// Right → Left → Bottom → Top, or hiding it if no side is clear.
#[allow(clippy::too_many_arguments)]
fn draw_milestone_label(
    canvas: &Canvas,
    rows: &[GanttRow],
    row_idx: usize,
    cx: f32,
    cy: f32,
    half: f32,
    label: &str,
    color: u32,
    font: &skia_safe::Font,
    metrics: &skia_safe::FontMetrics,
    view_start: NaiveDate,
    zoom: f32,
    scroll_x: f32,
) {
    let tw = font.measure_str(label, None).0;
    let pad = 4.0_f32;
    // Baseline that vertically centres single-line text at `cy`.
    let vc = cy - (metrics.ascent + metrics.descent) / 2.0;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(color));
    paint.set_style(PaintStyle::Fill);

    // Right — text starts at right edge of diamond
    let r_x = cx + half + pad;
    if !row_has_item_in_range(rows, row_idx, r_x, r_x + tw, view_start, zoom, scroll_x) {
        if let Some(blob) = TextBlob::new(label, font) {
            canvas.draw_text_blob(&blob, (r_x, vc), &paint);
        }
        return;
    }

    // Left — text ends at left edge of diamond
    let l_x = cx - half - pad - tw;
    if !row_has_item_in_range(rows, row_idx, l_x, l_x + tw, view_start, zoom, scroll_x) {
        if let Some(blob) = TextBlob::new(label, font) {
            canvas.draw_text_blob(&blob, (l_x, vc), &paint);
        }
        return;
    }

    // Bottom — text centred below diamond, checks row below
    let b_x = cx - tw / 2.0;
    if !row_has_item_in_range(rows, row_idx + 1, b_x, b_x + tw, view_start, zoom, scroll_x) {
        let by = cy + half + 2.0 - metrics.ascent;
        if let Some(blob) = TextBlob::new(label, font) {
            canvas.draw_text_blob(&blob, (b_x, by), &paint);
        }
        return;
    }

    // Top — text centred above diamond, checks row above (skip for row 0)
    if row_idx > 0 {
        let t_x = cx - tw / 2.0;
        if !row_has_item_in_range(rows, row_idx - 1, t_x, t_x + tw, view_start, zoom, scroll_x) {
            let ty = cy - half - 2.0 - metrics.descent;
            if let Some(blob) = TextBlob::new(label, font) {
                canvas.draw_text_blob(&blob, (t_x, ty), &paint);
            }
        }
    }
    // If no position is clear, the label is hidden.
}

struct ItemPos {
    start_x: f32,
    end_x: f32,
    center_y: f32,
}

fn build_item_pos_map(
    rows: &[GanttRow],
    height: f32,
    scroll_y: f32,
    zoom: f32,
    scroll_x: f32,
    view_start: NaiveDate,
) -> HashMap<NodeId, ItemPos> {
    let mut pos_map: HashMap<NodeId, ItemPos> = HashMap::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let cy = row_top_y(row_idx, rows.len(), height, scroll_y) + GANTT_ROW_H / 2.0;
        for item in &row.items {
            match item {
                GanttItem::Task { id, start, end } => {
                    pos_map.insert(
                        NodeId::Task(*id),
                        ItemPos {
                            // Connect arrows at the centre of the first/last day columns.
                            start_x: date_to_x(*start, view_start, zoom, scroll_x) + zoom / 2.0,
                            end_x: date_to_x(*end, view_start, zoom, scroll_x) + zoom / 2.0,
                            center_y: cy,
                        },
                    );
                }
                GanttItem::Milestone { id, date } => {
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    pos_map.insert(
                        NodeId::Milestone(*id),
                        ItemPos {
                            start_x: cx,
                            end_x: cx,
                            center_y: cy,
                        },
                    );
                }
                GanttItem::PlanStart { date } => {
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    pos_map.insert(
                        NodeId::PlanStart,
                        ItemPos {
                            start_x: cx - GANTT_MS_HALF * 1.1,
                            end_x: cx + GANTT_MS_HALF * 1.1,
                            center_y: cy,
                        },
                    );
                }
            }
        }
    }
    pos_map
}

fn draw_gantt_dependencies(
    canvas: &Canvas,
    state: &OverviewState,
    plan: &Plan,
    rows: &[GanttRow],
    width: f32,
    height: f32,
    view_start: NaiveDate,
) {
    let rows_top = gantt_rows_top();
    let scroll_y = state.scroll_y;
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top, width, height - rows_top),
        ClipOp::Intersect,
        false,
    );

    let has_hover = state.hovered_node.is_some();
    let arrow_color = if has_hover {
        GANTT_DEP_LINE_DIMMED
    } else {
        GANTT_DEP_LINE_COLOR
    };

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(arrow_color));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);

    let pos_map = build_item_pos_map(rows, height, scroll_y, zoom, scroll_x, view_start);
    let radius = 6.0f32;

    for task in plan.tasks.values() {
        let to_id = NodeId::Task(task.id);
        let to_pos = match pos_map.get(&to_id) {
            Some(p) => p,
            None => continue,
        };
        for dep in &task.dependencies {
            let from_id = dep.id;
            if has_hover
                && (state.hovered_node == Some(from_id) || state.hovered_node == Some(to_id))
            {
                continue;
            }
            if let Some(from_pos) = pos_map.get(&from_id) {
                draw_dep_arrow(
                    canvas,
                    &mut paint,
                    from_pos.end_x,
                    from_pos.center_y,
                    to_pos.start_x,
                    to_pos.center_y,
                    radius,
                    0.0,
                );
            }
        }
    }

    for ms in plan.milestones.values() {
        let to_id = NodeId::Milestone(ms.id);
        let to_pos = match pos_map.get(&to_id) {
            Some(p) => p,
            None => continue,
        };
        for dep in &ms.dependencies {
            let from_id = dep.id;
            if has_hover
                && (state.hovered_node == Some(from_id) || state.hovered_node == Some(to_id))
            {
                continue;
            }
            if let Some(from_pos) = pos_map.get(&from_id) {
                draw_dep_arrow(
                    canvas,
                    &mut paint,
                    from_pos.end_x,
                    from_pos.center_y,
                    to_pos.start_x,
                    to_pos.center_y,
                    radius,
                    0.0,
                );
            }
        }
    }

    canvas.restore();
}

fn draw_highlighted_dep_arrows(
    canvas: &Canvas,
    state: &OverviewState,
    plan: &Plan,
    rows: &[GanttRow],
    width: f32,
    height: f32,
    view_start: NaiveDate,
) {
    let rows_top = gantt_rows_top();
    let scroll_y = state.scroll_y;
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top, width, height - rows_top),
        ClipOp::Intersect,
        false,
    );

    let pos_map = build_item_pos_map(rows, height, scroll_y, zoom, scroll_x, view_start);
    let radius = 6.0f32;

    // Collect highlighted arrows into upstream (into hovered) and downstream (out of hovered).
    struct Arrow {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    }
    let mut upstream: Vec<Arrow> = Vec::new();
    let mut downstream: Vec<Arrow> = Vec::new();

    let collect = |deps: &[plinko_shared::data::dependency::Dependency],
                   to_id: NodeId,
                   to_pos: &ItemPos,
                   upstream: &mut Vec<Arrow>,
                   downstream: &mut Vec<Arrow>| {
        for dep in deps {
            let from_id = dep.id;
            let Some(from_pos) = pos_map.get(&from_id) else {
                continue;
            };
            let arrow = Arrow {
                from_x: from_pos.end_x,
                from_y: from_pos.center_y,
                to_x: to_pos.start_x,
                to_y: to_pos.center_y,
            };
            if state.hovered_node == Some(to_id) {
                upstream.push(arrow);
            } else if state.hovered_node == Some(from_id) {
                downstream.push(arrow);
            }
        }
    };

    for task in plan.tasks.values() {
        let to_id = NodeId::Task(task.id);
        if let Some(to_pos) = pos_map.get(&to_id) {
            collect(
                &task.dependencies,
                to_id,
                to_pos,
                &mut upstream,
                &mut downstream,
            );
        }
    }
    for ms in plan.milestones.values() {
        let to_id = NodeId::Milestone(ms.id);
        if let Some(to_pos) = pos_map.get(&to_id) {
            collect(
                &ms.dependencies,
                to_id,
                to_pos,
                &mut upstream,
                &mut downstream,
            );
        }
    }

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);

    // Draw all glows first, then all cores, per direction.
    // Upstream (dependency) arrows are offset slightly left; downstream slightly right
    // so they remain visually distinct when they share the same column.
    const HOVER_X_OFFSET: f32 = 4.0;
    for (arrows, glow_color, core_color, x_off) in [
        (
            &upstream,
            HOVER_ARROW_UPSTREAM_GLOW,
            HOVER_ARROW_UPSTREAM,
            -HOVER_X_OFFSET,
        ),
        (
            &downstream,
            HOVER_ARROW_DOWNSTREAM_GLOW,
            HOVER_ARROW_DOWNSTREAM,
            HOVER_X_OFFSET,
        ),
    ] {
        paint.set_stroke_width(3.5);
        paint.set_color(Color::from(glow_color));
        for a in arrows.iter() {
            draw_dep_arrow(
                canvas, &mut paint, a.from_x, a.from_y, a.to_x, a.to_y, radius, x_off,
            );
        }
        paint.set_stroke_width(1.8);
        paint.set_color(Color::from(core_color));
        for a in arrows.iter() {
            draw_dep_arrow(
                canvas, &mut paint, a.from_x, a.from_y, a.to_x, a.to_y, radius, x_off,
            );
        }
    }

    canvas.restore();
}

fn draw_dep_arrow(
    canvas: &Canvas,
    paint: &mut Paint,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    radius: f32,
    // Horizontal offset applied to the midpoint vertical segment.
    // Negative = shift left (upstream/dependency arrows in hover mode),
    // Positive = shift right (downstream/dependent arrows in hover mode).
    x_offset: f32,
) {
    if (y1 - y2).abs() < 1.0 {
        // Same row: simple horizontal line
        let mut pb = PathBuilder::new();
        pb.move_to((x1, y1));
        pb.line_to((x2, y2));
        canvas.draw_path(&pb.detach(), paint);
        draw_arrowhead(canvas, paint, x2, y2, false);
        return;
    }

    let sign_y = if y2 > y1 { 1.0 } else { -1.0 };
    let r = radius.min(((y2 - y1).abs() / 2.0).max(2.0));

    if x2 >= x1 + 2.0 * r {
        // Destination is comfortably to the right: route via midpoint S-curve.
        let mid_x = ((x1 + x2) / 2.0 + x_offset).clamp(x1 + r, x2 - r);
        let mut pb = PathBuilder::new();
        pb.move_to((x1, y1));
        pb.line_to((mid_x - r, y1));
        pb.cubic_to((mid_x, y1), (mid_x, y1), (mid_x, y1 + sign_y * r));
        pb.line_to((mid_x, y2 - sign_y * r));
        pb.cubic_to((mid_x, y2), (mid_x, y2), (mid_x + r, y2));
        pb.line_to((x2, y2));
        canvas.draw_path(&pb.detach(), paint);
        draw_arrowhead(canvas, paint, x2, y2, false);
    } else {
        // Destination is close or to the left: drop straight down (or up) from x1
        // and arrive with a downward (or upward) arrowhead. This avoids routing
        // back across the destination bar.
        let mut pb = PathBuilder::new();
        pb.move_to((x1, y1));
        pb.line_to((x1, y2 - sign_y * r));
        pb.cubic_to((x1, y2), (x1, y2), (x1 + r.min(4.0), y2));
        pb.line_to((x1 + r.min(4.0), y2));
        canvas.draw_path(&pb.detach(), paint);
        draw_arrowhead(canvas, paint, x1, y2, true);
    }
}

fn draw_arrowhead(canvas: &Canvas, paint: &mut Paint, x: f32, y: f32, vertical: bool) {
    let size = 5.0f32;
    let saved_style = paint.style();
    paint.set_style(PaintStyle::Fill);
    let mut pb = PathBuilder::new();
    if vertical {
        // Arrowhead pointing downward (or upward if the path came from below, but
        // we always drop downward in the vertical case)
        pb.move_to((x, y));
        pb.line_to((x - size / 2.0, y - size));
        pb.line_to((x + size / 2.0, y - size));
    } else {
        // Arrowhead pointing rightward
        pb.move_to((x, y));
        pb.line_to((x - size, y - size / 2.0));
        pb.line_to((x - size, y + size / 2.0));
    }
    pb.close();
    canvas.draw_path(&pb.detach(), paint);
    paint.set_style(saved_style);
}

// ── Hit testing ────────────────────────────────────────────────────────────────

/// Returns the Gantt item (task or milestone) under pixel coordinates `(x, y)`,
/// or `None` if the click was on empty space or the Plan Start marker.
pub fn hit_test_gantt_item(
    x: f32,
    y: f32,
    rows: &[GanttRow],
    state: &OverviewState,
    height: f32,
    view_start: NaiveDate,
) -> Option<GanttHit> {
    let zoom = state.zoom;
    let scroll_x = state.scroll_x;
    let scroll_y = state.scroll_y;
    let num_rows = rows.len();

    for (row_idx, row) in rows.iter().enumerate() {
        let row_y = row_top_y(row_idx, num_rows, height, scroll_y);

        for item in &row.items {
            match item {
                GanttItem::Task { id, start, end } => {
                    let bar_x = date_to_x(*start, view_start, zoom, scroll_x);
                    let bar_w = (((*end - *start).num_days() + 1) as f32 * zoom).max(4.0);
                    let bar_y = row_y + GANTT_ROW_PADDING;
                    let bar_h = GANTT_ROW_H - 2.0 * GANTT_ROW_PADDING;

                    if x >= bar_x && x <= bar_x + bar_w && y >= bar_y && y <= bar_y + bar_h {
                        return Some(GanttHit::Task(*id));
                    }
                }
                GanttItem::Milestone { id, date } => {
                    let cx = date_to_x(*date, view_start, zoom, scroll_x) + zoom / 2.0;
                    let cy = row_y + GANTT_ROW_H / 2.0;
                    // Diamond hit: Manhattan distance from centre.
                    if (x - cx).abs() + (y - cy).abs() <= GANTT_MS_HALF * 1.5 {
                        return Some(GanttHit::Milestone(*id));
                    }
                }
                // Plan Start marker is not editable.
                GanttItem::PlanStart { .. } => {}
            }
        }
    }

    None
}

fn draw_node_info_panel(
    canvas: &Canvas,
    node_id: NodeId,
    plan: &Plan,
    _width: f32,
    height: f32,
    cache: &RenderCache,
) {
    let pad = 10.0_f32;
    let line_gap = 3.0_f32;
    let margin = 8.0_f32;

    // First line (name) uses the larger title font; the rest use the normal font.
    let (_, title_metrics) = cache.title_font.metrics();
    let title_line_h = (title_metrics.descent - title_metrics.ascent).ceil();
    let (_, metrics) = cache.font.metrics();
    let line_h = (metrics.descent - metrics.ascent).ceil();

    let mut lines: Vec<String> = Vec::new();

    match node_id {
        NodeId::Task(id) => {
            if let Some(task) = plan.tasks.get(&id) {
                lines.push(task.name.clone());
                let status = plan.task_status(&id);
                lines.push(format!("Status: {:?}", status));
                if let Some(start) = plan.task_actual_start(&id) {
                    lines.push(format!("Started: {}", start));
                } else if let Some(start_date) = plan
                    .node_allocations
                    .tasks
                    .get(&id)
                    .map(|s| s.allocation.start_date())
                {
                    lines.push(format!("Scheduled: {}", start_date));
                }
                if let Some(end) = plan.task_actual_end(&id) {
                    lines.push(format!("Ended: {}", end));
                }
                // Collect users that are actually allocated via segments
                // (used to resolve Placeholder slots).
                let placeholder_workers: Vec<UserId> = {
                    let specific_ids: std::collections::HashSet<UserId> = task
                        .workers
                        .iter()
                        .filter_map(|s| {
                            if let plinko_shared::data::task::WorkerSlot::Specific {
                                user_id, ..
                            } = s
                            {
                                Some(*user_id)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if let Some(ts) = plan.node_allocations.tasks.get(&id) {
                        let segs = match &ts.allocation {
                            plinko_shared::data::TaskAllocation::Dynamic {
                                time_allocation,
                                ..
                            }
                            | plinko_shared::data::TaskAllocation::Fixed {
                                time_allocation, ..
                            } => time_allocation,
                        };
                        let mut seen: Vec<UserId> = Vec::new();
                        for s in segs {
                            if !specific_ids.contains(&s.user) && !seen.contains(&s.user) {
                                seen.push(s.user);
                            }
                        }
                        seen
                    } else {
                        Vec::new()
                    }
                };
                let mut placeholder_idx = 0;
                let worker_names: Vec<String> = task
                    .workers
                    .iter()
                    .filter_map(|slot| match slot {
                        plinko_shared::data::task::WorkerSlot::Specific { user_id, .. } => {
                            plan.user(user_id).map(|u| u.name.clone())
                        }
                        plinko_shared::data::task::WorkerSlot::Placeholder {
                            required_tags,
                            ..
                        } => {
                            // Resolve tag names for this slot (shown as suffix).
                            let tag_names: Vec<&str> = required_tags
                                .iter()
                                .filter_map(|tid| plan.tags.iter().find(|t| &t.id == tid))
                                .map(|t| t.name.as_str())
                                .collect();
                            let tag_suffix = if tag_names.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", tag_names.join(", "))
                            };
                            // Use the resolved user for this placeholder if available.
                            if placeholder_idx < placeholder_workers.len() {
                                let uid = placeholder_workers[placeholder_idx];
                                placeholder_idx += 1;
                                plan.user(&uid).map(|u| format!("{}{}", u.name, tag_suffix))
                            } else {
                                // Fallback: show tag requirements only.
                                if tag_names.is_empty() {
                                    Some(String::from("(unassigned)"))
                                } else {
                                    Some(format!("needs: {}", tag_names.join(", ")))
                                }
                            }
                        }
                    })
                    .collect();
                if !worker_names.is_empty() {
                    lines.push(format!("Workers: {}", worker_names.join(", ")));
                }
            }
        }
        NodeId::Milestone(id) => {
            if let Some(ms) = plan.milestones.get(&id) {
                lines.push(ms.name.clone());
                lines.push(String::from("Milestone"));
                if let Some(state) = plan.node_allocations.milestones.get(&id) {
                    lines.push(format!("Scheduled: {}", state.date));
                }
            }
        }
        NodeId::PlanStart => {
            lines.push(String::from("Plan Start"));
            lines.push(format!("Date: {}", plan.start_date));
        }
    }

    if lines.is_empty() {
        return;
    }

    // Measure: name (title_font) + rest (font)
    let name_w = cache.title_font.measure_str(lines[0].as_str(), None).0;
    let rest_w = lines[1..]
        .iter()
        .map(|l| cache.font.measure_str(l.as_str(), None).0)
        .fold(0.0_f32, f32::max);
    let max_w = name_w.max(rest_w);

    let panel_w = max_w + pad * 2.0;
    let panel_h = title_line_h
        + if lines.len() > 1 {
            line_gap + (lines.len() - 1) as f32 * line_h + (lines.len() - 2) as f32 * line_gap
        } else {
            0.0
        }
        + pad * 2.0;
    let px = margin;
    let py = height - margin - panel_h;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(SHADOW_COLOR));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(px + 2.0, py + 3.0, panel_w, panel_h),
            6.0,
            6.0,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(TOOLTIP_BG));
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

    // Name (first line) in title_font
    // `ascent` is negative (above baseline), so `-ascent` = |ascent|.
    // This places the *top* of the text at `py + pad`.
    if let Some(blob) = TextBlob::new(lines[0].as_str(), &cache.title_font) {
        paint.set_color(Color::from(INPUT_FG));
        let ty = py + pad - title_metrics.ascent;
        canvas.draw_text_blob(&blob, (px + pad, ty), &paint);
    }

    // Remaining lines in normal font
    for (i, line) in lines[1..].iter().enumerate() {
        if let Some(blob) = TextBlob::new(line.as_str(), &cache.font) {
            paint.set_color(Color::from(MUTED_FG));
            let ty = py + pad + title_line_h + line_gap + i as f32 * (line_h + line_gap)
                - metrics.ascent;
            canvas.draw_text_blob(&blob, (px + pad, ty), &paint);
        }
    }
}
