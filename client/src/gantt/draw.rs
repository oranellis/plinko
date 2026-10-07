//! Painting the Gantt on a canvas. Everything here is drawn from the `View` and the rows, with
//! no state of its own.

use chrono::{Datelike, NaiveDate, Weekday};
use plinko::types::dependency::PlanNode;
use web_sys::CanvasRenderingContext2d;

use crate::{
    format,
    gantt::layout::{HEADER_HEIGHT, ROW_HEIGHT, View, bar, date_of, day_number},
    model::{Look, Row},
};

pub const LABEL_WIDTH: f64 = 220.0;
pub const LABEL_WIDTH_NARROW: f64 = 120.0;
const INDENT: f64 = 14.0;

const BACKGROUND: &str = "#ffffff";
const STRIPE: &str = "#f8fafc";
const WEEKEND: &str = "#f1f5f9";
const GRID: &str = "#e2e8f0";
const TEXT: &str = "#0f172a";
const MUTED: &str = "#64748b";
const SELECTED: &str = "#dbeafe";
const TODAY: &str = "#ef4444";
const PROBLEM: &str = "#dc2626";

fn colour(look: Look) -> &'static str {
    match look {
        Look::NotStarted => "#3b82f6",
        Look::InProgress => "#f59e0b",
        Look::OnHold => "#94a3b8",
        Look::Completed => "#22c55e",
    }
}

pub struct Scene<'a> {
    pub width: f64,
    pub height: f64,
    pub label_width: f64,
    pub view: View,
    pub rows: &'a [Row],
    pub selected: Option<PlanNode>,
    pub today: NaiveDate,
}

fn rect(ctx: &CanvasRenderingContext2d, style: &str, x: f64, y: f64, w: f64, h: f64) {
    ctx.set_fill_style_str(style);
    ctx.fill_rect(x, y, w, h);
}

fn line(ctx: &CanvasRenderingContext2d, style: &str, x1: f64, y1: f64, x2: f64, y2: f64) {
    ctx.set_stroke_style_str(style);
    ctx.begin_path();
    ctx.move_to(x1, y1);
    ctx.line_to(x2, y2);
    ctx.stroke();
}

fn text(ctx: &CanvasRenderingContext2d, style: &str, words: &str, x: f64, y: f64) {
    ctx.set_fill_style_str(style);
    let _ = ctx.fill_text(words, x, y);
}

pub fn paint(ctx: &CanvasRenderingContext2d, scene: &Scene) {
    let Scene {
        width,
        height,
        label_width,
        ..
    } = *scene;
    rect(ctx, BACKGROUND, 0.0, 0.0, width, height);
    ctx.set_font("12px system-ui, sans-serif");
    ctx.set_text_baseline("middle");
    ctx.set_line_width(1.0);

    ctx.save();
    ctx.translate(label_width, 0.0).ok();
    ctx.begin_path();
    ctx.rect(0.0, 0.0, width - label_width, height);
    ctx.clip();
    let timeline = width - label_width;
    paint_rows(ctx, scene, timeline);
    paint_header(ctx, scene, timeline);
    paint_today(ctx, scene);
    ctx.restore();
    paint_labels(ctx, scene);
}

fn paint_header(ctx: &CanvasRenderingContext2d, scene: &Scene, timeline: f64) {
    let view = scene.view;
    rect(ctx, BACKGROUND, 0.0, 0.0, timeline, HEADER_HEIGHT);
    line(ctx, GRID, 0.0, HEADER_HEIGHT, timeline, HEADER_HEIGHT);
    let narrow = view.day_width < 14.0;
    for day in view.days(timeline) {
        let Some(date) = date_of(day as f64) else {
            continue;
        };
        let x = view.x(day as f64);
        let first_of_month = date.day() == 1;
        if first_of_month || (day as f64 - view.origin.floor()).abs() < 0.5 {
            text(
                ctx,
                TEXT,
                &format!("{} {}", format::month(date), date.year()),
                x.max(2.0) + 4.0,
                12.0,
            );
        }
        if first_of_month {
            line(ctx, MUTED, x, 0.0, x, HEADER_HEIGHT);
        }
        if narrow {
            if date.weekday() == Weekday::Mon {
                text(ctx, MUTED, &date.day().to_string(), x + 2.0, 32.0);
            }
        } else {
            let label = if view.day_width > 40.0 {
                format!("{} {}", format::weekday(date), date.day())
            } else {
                date.day().to_string()
            };
            text(ctx, MUTED, &label, x + 3.0, 32.0);
        }
    }
}

fn paint_today(ctx: &CanvasRenderingContext2d, scene: &Scene) {
    let x = scene.view.x(day_number(scene.today) + 0.5);
    line(ctx, TODAY, x, 0.0, x, scene.height);
}

fn paint_rows(ctx: &CanvasRenderingContext2d, scene: &Scene, timeline: f64) {
    let view = scene.view;
    // weekends and grid behind everything
    for day in view.days(timeline) {
        let Some(date) = date_of(day as f64) else {
            continue;
        };
        let x = view.x(day as f64);
        if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            rect(ctx, WEEKEND, x, HEADER_HEIGHT, view.day_width, scene.height);
        }
        if view.day_width >= 14.0 {
            line(ctx, GRID, x, HEADER_HEIGHT, x, scene.height);
        }
    }
    for index in view.visible_rows(scene.height, scene.rows.len()) {
        let row = &scene.rows[index];
        let top = view.row_top(index);
        if Some(row.node) == scene.selected {
            rect(ctx, SELECTED, 0.0, top, timeline, ROW_HEIGHT);
        } else if index % 2 == 1 {
            ctx.set_global_alpha(0.5);
            rect(ctx, STRIPE, 0.0, top, timeline, ROW_HEIGHT);
            ctx.set_global_alpha(1.0);
        }
        line(ctx, GRID, 0.0, top + ROW_HEIGHT, timeline, top + ROW_HEIGHT);
        if let Some(span) = row.span {
            paint_bar(ctx, scene, row, span, top);
        }
    }
}

fn paint_bar(
    ctx: &CanvasRenderingContext2d,
    scene: &Scene,
    row: &Row,
    span: (NaiveDate, NaiveDate),
    top: f64,
) {
    let (x, width) = bar(&scene.view, span);
    let middle = top + ROW_HEIGHT / 2.0;
    match row.node {
        PlanNode::Milestone(_) => {
            let centre = x + scene.view.day_width / 2.0;
            ctx.set_fill_style_str(colour(row.look));
            ctx.begin_path();
            ctx.move_to(centre, middle - 9.0);
            ctx.line_to(centre + 9.0, middle);
            ctx.line_to(centre, middle + 9.0);
            ctx.line_to(centre - 9.0, middle);
            ctx.close_path();
            ctx.fill();
            if row.violated {
                ctx.set_stroke_style_str(PROBLEM);
                ctx.set_line_width(2.0);
                ctx.stroke();
                ctx.set_line_width(1.0);
            }
        }
        PlanNode::TaskGroup(_) => {
            rect(ctx, "#334155", x, top + 9.0, width, 5.0);
            rect(ctx, "#334155", x, top + 9.0, 3.0, 12.0);
            rect(ctx, "#334155", x + width - 3.0, top + 9.0, 3.0, 12.0);
        }
        PlanNode::PlanBuffer(_) => {
            rect(ctx, "#cbd5e1", x, top + 8.0, width, ROW_HEIGHT - 16.0);
            ctx.set_stroke_style_str("#94a3b8");
            ctx.stroke_rect(x, top + 8.0, width, ROW_HEIGHT - 16.0);
        }
        PlanNode::Task(_) => {
            rect(
                ctx,
                colour(row.look),
                x,
                top + 6.0,
                width,
                ROW_HEIGHT - 12.0,
            );
            if row.violated {
                ctx.set_stroke_style_str(PROBLEM);
                ctx.set_line_width(2.0);
                ctx.stroke_rect(x, top + 6.0, width, ROW_HEIGHT - 12.0);
                ctx.set_line_width(1.0);
            }
            if width > 60.0 {
                text(
                    ctx,
                    "#ffffff",
                    &clip(ctx, &row.name, width - 8.0),
                    x + 5.0,
                    middle,
                );
            }
        }
    }
}

/// The name cut short with an ellipsis if it is wider than `room`
fn clip(ctx: &CanvasRenderingContext2d, name: &str, room: f64) -> String {
    let wide = |s: &str| ctx.measure_text(s).map(|m| m.width()).unwrap_or(0.0);
    if wide(name) <= room {
        return name.to_string();
    }
    let mut kept: String = name.to_string();
    while !kept.is_empty() && wide(&format!("{kept}…")) > room {
        kept.pop();
    }
    format!("{kept}…")
}

fn paint_labels(ctx: &CanvasRenderingContext2d, scene: &Scene) {
    let (label_width, view) = (scene.label_width, scene.view);
    ctx.save();
    ctx.begin_path();
    ctx.rect(0.0, 0.0, label_width, scene.height);
    ctx.clip();
    rect(ctx, BACKGROUND, 0.0, 0.0, label_width, scene.height);
    for index in view.visible_rows(scene.height, scene.rows.len()) {
        let row = &scene.rows[index];
        let top = view.row_top(index);
        if Some(row.node) == scene.selected {
            rect(ctx, SELECTED, 0.0, top, label_width, ROW_HEIGHT);
        }
        let indent = 8.0 + f64::from(row.depth) * INDENT;
        let mut x = indent;
        if row.has_members {
            text(
                ctx,
                MUTED,
                if row.collapsed { "▸" } else { "▾" },
                x,
                top + ROW_HEIGHT / 2.0,
            );
            x += 12.0;
        }
        let problem = row.unscheduled.is_some() || row.violated;
        let name = clip(
            ctx,
            &row.name,
            label_width - x - if problem { 26.0 } else { 8.0 },
        );
        let style = if row.unscheduled.is_some() {
            MUTED
        } else {
            TEXT
        };
        text(ctx, style, &name, x, top + ROW_HEIGHT / 2.0);
        if problem {
            text(
                ctx,
                PROBLEM,
                "⚠",
                label_width - 20.0,
                top + ROW_HEIGHT / 2.0,
            );
        }
        line(
            ctx,
            GRID,
            0.0,
            top + ROW_HEIGHT,
            label_width,
            top + ROW_HEIGHT,
        );
    }
    rect(ctx, BACKGROUND, 0.0, 0.0, label_width, HEADER_HEIGHT);
    text(ctx, MUTED, "Name", 8.0, 22.0);
    line(ctx, GRID, 0.0, HEADER_HEIGHT, label_width, HEADER_HEIGHT);
    ctx.restore();
    line(ctx, GRID, label_width, 0.0, label_width, scene.height);
}
