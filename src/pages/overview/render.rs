//! Rendering functions for the overview page Gantt chart.

use chrono::{Datelike, Duration, NaiveDate, Weekday as CWeekday};
use skia_safe::{Canvas, ClipOp, Color, Paint, PaintStyle, PathBuilder, RRect, Rect, TextBlob};

use crate::data::Plan;
use crate::data::task::TaskStatus;
use crate::ui::cache::RenderCache;
use crate::ui::icon_button;
use crate::ui::layout::*;

use super::gantt::{
    GanttItem, GanttRow, MilestoneStatus, compute_date_range, milestone_display_status, pack_rows,
};
use super::state::OverviewState;

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

fn view_start_date(plan: &Plan) -> NaiveDate {
    compute_date_range(plan)
        .map(|(s, _)| s)
        .unwrap_or(plan.start_date)
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

    // Draw in layered order: backgrounds → grid lines → items.
    draw_gantt_row_backgrounds(canvas, state, &rows, w, h);
    draw_gantt_grid(canvas, state, w, h, view_start);
    draw_gantt_rows(canvas, state, plan, &rows, w, h, view_start, cache);
    draw_gantt_dependencies(canvas, state, plan, &rows, w, h, view_start);
    draw_gantt_header(canvas, state, w, view_start, cache);
    draw_toolbar_buttons(canvas, state, cache, w);
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

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, rows_top, width, height - rows_top),
        ClipOp::Intersect,
        false,
    );

    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_style(PaintStyle::Fill);

    for (row_idx, _) in rows.iter().enumerate() {
        if row_idx % 2 == 1 {
            let row_y = rows_top + row_idx as f32 * GANTT_ROW_H - scroll_y;
            paint.set_color(Color::from(GANTT_ROW_ALT_BG));
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
        let row_y = rows_top + row_idx as f32 * GANTT_ROW_H - scroll_y;

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

                    let bar_color = task_status_color(task.status);
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
                }

                GanttItem::Milestone { id, date } => {
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

                    paint.set_color(Color::from(ms_color));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_path(&ms_path, &paint);

                    paint.set_color(Color::from(darken(ms_color)));
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&ms_path, &paint);
                    paint.set_style(PaintStyle::Fill);

                    if let Some(ms) = plan.milestones.get(id) {
                        paint.set_color(Color::from(GANTT_HEADER_FG));
                        let name_y = cy + half + 2.0 - metrics.ascent;
                        if let Some(blob) = TextBlob::new(&ms.name, &cache.font) {
                            let tw = cache.font.measure_str(&ms.name, None).0;
                            canvas.draw_text_blob(&blob, (cx - tw / 2.0, name_y), &paint);
                        }
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

                    paint.set_color(Color::from(GANTT_PLAN_START_COLOR));
                    let label = "Plan Start";
                    let tw = cache.font.measure_str(label, None).0;
                    let name_y = cy + half + 2.0 - metrics.ascent;
                    if let Some(blob) = TextBlob::new(label, &cache.font) {
                        canvas.draw_text_blob(&blob, (cx - tw / 2.0, name_y), &paint);
                    }
                }
            }
        }
    }

    canvas.restore();
}

fn task_status_color(status: TaskStatus) -> u32 {
    match status {
        TaskStatus::NotStarted => GANTT_TASK_NOT_STARTED,
        TaskStatus::InProgress => GANTT_TASK_IN_PROGRESS,
        TaskStatus::OnHold => GANTT_TASK_ON_HOLD,
        TaskStatus::Complete => GANTT_TASK_COMPLETE,
        TaskStatus::Dropped => GANTT_TASK_DROPPED,
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

// ── Dependency lines ───────────────────────────────────────────────────────────

fn draw_gantt_dependencies(
    canvas: &Canvas,
    state: &OverviewState,
    plan: &Plan,
    rows: &[GanttRow],
    width: f32,
    height: f32,
    view_start: NaiveDate,
) {
    use crate::data::ids::NodeId;
    use std::collections::HashMap;

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

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(GANTT_DEP_LINE_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);

    struct ItemPos {
        start_x: f32,
        end_x: f32,
        center_y: f32,
    }

    let mut pos_map: HashMap<NodeId, ItemPos> = HashMap::new();

    for (row_idx, row) in rows.iter().enumerate() {
        let cy = rows_top + row_idx as f32 * GANTT_ROW_H + GANTT_ROW_H / 2.0 - scroll_y;
        for item in &row.items {
            match item {
                GanttItem::Task { id, start, end } => {
                    pos_map.insert(
                        NodeId::Task(*id),
                        ItemPos {
                            start_x: date_to_x(*start, view_start, zoom, scroll_x),
                            end_x: date_to_x(*end, view_start, zoom, scroll_x) + zoom,
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
                GanttItem::PlanStart { .. } => {}
            }
        }
    }

    let radius = 6.0f32;

    for task in plan.tasks.values() {
        let to_pos = match pos_map.get(&NodeId::Task(task.id)) {
            Some(p) => p,
            None => continue,
        };
        for dep in &task.dependencies {
            if dep.id == NodeId::PlanStart {
                continue;
            }
            if let Some(from_pos) = pos_map.get(&dep.id) {
                draw_dep_arrow(
                    canvas,
                    &mut paint,
                    from_pos.end_x,
                    from_pos.center_y,
                    to_pos.start_x,
                    to_pos.center_y,
                    radius,
                );
            }
        }
    }

    for ms in plan.milestones.values() {
        let to_pos = match pos_map.get(&NodeId::Milestone(ms.id)) {
            Some(p) => p,
            None => continue,
        };
        for dep in &ms.dependencies {
            if dep.id == NodeId::PlanStart {
                continue;
            }
            if let Some(from_pos) = pos_map.get(&dep.id) {
                draw_dep_arrow(
                    canvas,
                    &mut paint,
                    from_pos.end_x,
                    from_pos.center_y,
                    to_pos.start_x,
                    to_pos.center_y,
                    radius,
                );
            }
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
) {
    if (y1 - y2).abs() < 1.0 {
        // Same row: simple horizontal line
        let mut pb = PathBuilder::new();
        pb.move_to((x1, y1));
        pb.line_to((x2, y2));
        canvas.draw_path(&pb.detach(), paint);
        draw_arrowhead(canvas, paint, x2, y2);
        return;
    }

    let r = radius.min(((y2 - y1).abs() / 2.0).max(2.0));
    let mid_x = if x2 > x1 + r * 2.0 {
        (x1 + x2) / 2.0
    } else {
        x1 + r * 2.0
    };
    let sign_y = if y2 > y1 { 1.0 } else { -1.0 };

    let mut pb = PathBuilder::new();
    pb.move_to((x1, y1));
    pb.line_to((mid_x - r, y1));
    pb.cubic_to((mid_x, y1), (mid_x, y1), (mid_x, y1 + sign_y * r));
    pb.line_to((mid_x, y2 - sign_y * r));
    pb.cubic_to((mid_x, y2), (mid_x, y2), (mid_x + r, y2));
    pb.line_to((x2, y2));
    canvas.draw_path(&pb.detach(), paint);
    draw_arrowhead(canvas, paint, x2, y2);
}

fn draw_arrowhead(canvas: &Canvas, paint: &mut Paint, x: f32, y: f32) {
    let size = 5.0f32;
    let saved_style = paint.style();
    paint.set_style(PaintStyle::Fill);
    let mut pb = PathBuilder::new();
    pb.move_to((x, y));
    pb.line_to((x - size, y - size / 2.0));
    pb.line_to((x - size, y + size / 2.0));
    pb.close();
    canvas.draw_path(&pb.detach(), paint);
    paint.set_style(saved_style);
}
