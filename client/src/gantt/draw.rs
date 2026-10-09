//! Painting the chart on a canvas, the way Plinko 1 did: dark, rounded bars with their names on
//! them, diamonds for milestones, groups as faint rounded boxes around their members, and the
//! dependencies of the item under the pointer as lines behind the bars. It draws from what it is
//! given and keeps no state, and it hands back where everything ended up so the pointer can hit it.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, Weekday};
use plinko::types::dependency::PlanNode;
use web_sys::CanvasRenderingContext2d;

use crate::{
    format,
    gantt::{
        arrange::{Kind, Mark, Placed},
        layout::{HEADER_HEIGHT, MONTH_HEIGHT, ROW_HEIGHT, Rect, Viewport},
        links::{Anchor, Link, Role},
        scene::Scene,
    },
    model::Look,
};

const BACKGROUND: &str = "#1e1e1e";
const HEADER: &str = "#252526";
const WEEKEND: &str = "#1e1e1e";
const BORDER: &str = "#3a3a3c";
const GRID: &str = "#2a2a2c";
const TODAY_BAND: &str = "rgba(64,91,200,0.18)";
const TODAY_TEXT: &str = "#4a90d9";
const TARGET: &str = "#ffd600";
const HOVERED: &str = "#1e88e5";
const BEFORE: &str = "#fc1ef1";
const AFTER: &str = "#07fcd7";
const BEFORE_LINE: &str = "rgba(252,30,241,0.66)";
const INSIDE: &str = "#1255a5";
const AFTER_LINE: &str = "rgba(7,252,215,0.66)";
const PROBLEM: &str = "#e65100";
const PLAN_START: &str = "#a78bfa";
const GROUP_FILL: &str = "rgba(255,255,255,0.035)";
const GROUP_LINE: &str = "#4a4a4c";
const RADIUS: f64 = 6.0;

fn look_colour(look: Look) -> &'static str {
    match look {
        Look::NotStarted => "#555555",
        Look::InProgress => "#f5a623",
        Look::OnHold => "#b39ddb",
        Look::Completed => "#66bb6a",
    }
}

fn milestone_colour(look: Look) -> &'static str {
    match look {
        Look::Completed => "#66bb6a",
        Look::InProgress => "#f5a623",
        _ => "#888888",
    }
}

/// Everything one frame needs
pub struct Frame<'a> {
    pub width: f64,
    pub height: f64,
    pub viewport: Viewport,
    pub scene: &'a Scene,
    pub today: NaiveDate,
    pub hover: Option<Mark>,
    pub links: &'a [Link],
    /// Something to outline in red, and how strongly
    pub flash: Option<(Mark, f64, &'static str)>,
    /// What to say about the item under the pointer, the title first
    pub info: &'a [String],
    /// A short note by the pointer for a violation under it, and where the pointer is
    pub tip: Option<(&'a [String], (f64, f64))>,
}

/// Where the things drawn are, for hit testing
#[derive(Default)]
pub struct Hits {
    pub items: Vec<(Mark, Rect)>,
    pub violations: Vec<(Mark, Rect)>,
    /// A group's name strip, and the small arrow in it that folds the group
    pub headers: Vec<(Mark, Rect, Rect)>,
}

/// Where an item or group sits: its first day, its last day, and for a group its box
struct Geometry {
    start: (f64, f64),
    end: (f64, f64),
}

fn path_round(ctx: &CanvasRenderingContext2d, r: Rect, radius: f64) {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    ctx.begin_path();
    ctx.move_to(r.x + radius, r.y);
    ctx.line_to(r.x + r.w - radius, r.y);
    ctx.quadratic_curve_to(r.x + r.w, r.y, r.x + r.w, r.y + radius);
    ctx.line_to(r.x + r.w, r.y + r.h - radius);
    ctx.quadratic_curve_to(r.x + r.w, r.y + r.h, r.x + r.w - radius, r.y + r.h);
    ctx.line_to(r.x + radius, r.y + r.h);
    ctx.quadratic_curve_to(r.x, r.y + r.h, r.x, r.y + r.h - radius);
    ctx.line_to(r.x, r.y + radius);
    ctx.quadratic_curve_to(r.x, r.y, r.x + radius, r.y);
    ctx.close_path();
}

fn path_diamond(ctx: &CanvasRenderingContext2d, cx: f64, cy: f64, r: f64) {
    ctx.begin_path();
    ctx.move_to(cx, cy - r);
    ctx.line_to(cx + r, cy);
    ctx.line_to(cx, cy + r);
    ctx.line_to(cx - r, cy);
    ctx.close_path();
}

fn text(ctx: &CanvasRenderingContext2d, colour: &str, font: &str, words: &str, x: f64, y: f64) {
    ctx.set_fill_style_str(colour);
    ctx.set_font(font);
    let _ = ctx.fill_text(words, x, y);
}

fn clip_rect(ctx: &CanvasRenderingContext2d, r: Rect) {
    ctx.begin_path();
    ctx.rect(r.x, r.y, r.w, r.h);
    ctx.clip();
}

/// The border an item gets for what it is to the item under the pointer or for being found
/// What is outlined besides the item under the pointer
struct Marked {
    before: Vec<Mark>,
    after: Vec<Mark>,
    /// Everything inside the group under the pointer
    inside: Vec<Mark>,
}

impl Marked {
    fn of(frame: &Frame) -> Self {
        let linked = |role: Role| frame.links.iter().filter(move |l| l.role == role);
        let placed = &frame.scene.arrangement.placed;
        let group = placed
            .iter()
            .find(|p| p.kind == Kind::Group && Some(p.mark) == frame.hover);
        Self {
            // a group's start is only marked by its diamond, the group itself is not before
            before: linked(Role::Before)
                .filter(|l| !matches!(l.from, Anchor::Start(PlanNode::TaskGroup(_))))
                .map(|l| mark_of(l.from))
                .collect(),
            // the end of a group is only marked by its diamond
            after: linked(Role::After)
                .filter(|l| !matches!(l.to, Anchor::End(PlanNode::TaskGroup(_))))
                .map(|l| mark_of(l.to))
                .collect(),
            // a group's block takes the lanes under its name
            inside: group
                .map(|g| {
                    placed
                        .iter()
                        .filter(|p| p.lane > g.lane && p.lane < g.lane + g.lanes)
                        .map(|p| p.mark)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

fn border(frame: &Frame, mark: Mark, target: bool, marked: &Marked) -> Option<(String, f64)> {
    match frame.flash {
        Some((flashed, alpha, rgb)) if flashed == mark => {
            return Some((format!("rgba({rgb},{alpha})"), 4.0));
        }
        _ => {}
    }
    if target {
        Some((TARGET.to_string(), 3.0))
    } else if frame.hover == Some(mark) {
        Some((HOVERED.to_string(), 3.5))
    } else if marked.inside.contains(&mark) {
        Some((INSIDE.to_string(), 3.0))
    } else if marked.before.contains(&mark) {
        Some((BEFORE.to_string(), 3.0))
    } else if marked.after.contains(&mark) {
        Some((AFTER.to_string(), 3.0))
    } else {
        None
    }
}

fn mark_of(anchor: Anchor) -> Mark {
    match anchor {
        Anchor::PlanStart => Mark::PlanStart,
        Anchor::Start(node) | Anchor::End(node) => Mark::Node(node),
    }
}

pub fn paint(ctx: &CanvasRenderingContext2d, frame: &Frame) -> Hits {
    let mut hits = Hits::default();
    ctx.set_fill_style_str(BACKGROUND);
    ctx.fill_rect(0.0, 0.0, frame.width, frame.height);
    ctx.set_line_width(1.0);

    let start = frame.scene.plan_start;
    let vp = frame.viewport;
    let offset = |date: NaiveDate| (date - start).num_days() as f64;
    let today_offset = offset(frame.today);

    let today_x = vp.x(today_offset);
    if today_x >= 0.0 && today_x < frame.width {
        ctx.set_fill_style_str(TODAY_BAND);
        ctx.fill_rect(today_x, 0.0, vp.day_width, frame.height);
    }
    paint_header(ctx, frame, start);

    // the rows, below the header
    ctx.save();
    clip_rect(
        ctx,
        Rect {
            x: 0.0,
            y: HEADER_HEIGHT,
            w: frame.width,
            h: frame.height - HEADER_HEIGHT,
        },
    );
    let marked = Marked::of(frame);

    // where everything is, so lines can be drawn to it
    let mut geometry: HashMap<Mark, Geometry> = HashMap::new();
    let mut group_boxes: HashMap<Mark, Rect> = HashMap::new();
    let visible = |p: &Placed| {
        let top = vp.lane_top(p.lane);
        top + ROW_HEIGHT * p.lanes as f64 > HEADER_HEIGHT && top < frame.height
    };
    for p in &frame.scene.arrangement.placed {
        let (from, to) = (offset(p.span.0), offset(p.span.1));
        if p.kind == Kind::Group {
            let bar = vp.bar(from, to, p.lane);
            let expand = 4.0 + 4.0 * f64::from(p.levels);
            // the top and bottom of nested groups share the lines of the grid, only the sides step out
            let inset = 2.0;
            let top = vp.lane_top(p.lane) + inset;
            let rect = Rect {
                x: bar.x - expand,
                y: top,
                w: bar.w + 2.0 * expand,
                h: ROW_HEIGHT * p.lanes as f64 - 2.0 * inset,
            };
            geometry.insert(
                p.mark,
                Geometry {
                    start: (rect.x, rect.y + rect.h / 2.0),
                    end: (rect.x + rect.w, rect.y + rect.h / 2.0),
                },
            );
            group_boxes.insert(p.mark, rect);
            if visible(p) {
                paint_group(ctx, frame, p, rect, &marked, &mut hits);
            }
        } else {
            geometry.insert(
                p.mark,
                Geometry {
                    start: vp.start_centre(from, p.lane),
                    end: vp.end_centre(to, p.lane),
                },
            );
        }
    }

    // the dependencies of the item under the pointer, behind the bars
    paint_links(ctx, frame, &geometry, &group_boxes);

    for p in &frame.scene.arrangement.placed {
        if p.kind == Kind::Group || !visible(p) {
            continue;
        }
        let (from, to) = (offset(p.span.0), offset(p.span.1));
        match (p.mark, p.kind) {
            (Mark::PlanStart, _) => paint_plan_start(ctx, frame, p, from, &marked, &mut hits),
            (_, Kind::Milestone) => paint_milestone(ctx, frame, p, from, &marked, &mut hits),
            _ => paint_bar(ctx, frame, p, (from, to), &marked, &mut hits),
        }
    }
    ctx.restore();

    if !frame.info.is_empty() {
        paint_panel(ctx, frame.info, frame.height);
    }
    if let Some((lines, pointer)) = frame.tip {
        paint_tip(ctx, lines, pointer, frame.width, frame.height);
    }
    hits
}

fn paint_header(ctx: &CanvasRenderingContext2d, frame: &Frame, start: NaiveDate) {
    let vp = frame.viewport;
    ctx.set_fill_style_str(HEADER);
    ctx.fill_rect(0.0, 0.0, frame.width, HEADER_HEIGHT);
    let (first, last) = vp.visible_days(frame.width);
    let today = (frame.today - start).num_days();
    let mut month_from: Option<NaiveDate> = None;
    let day_row = HEADER_HEIGHT - MONTH_HEIGHT;
    for offset in first..=last {
        let Some(date) = start.checked_add_signed(chrono::Duration::days(offset)) else {
            continue;
        };
        let x = vp.x(offset as f64);
        if date.day() == 1 {
            ctx.set_stroke_style_str(BORDER);
            ctx.begin_path();
            ctx.move_to(x, 0.0);
            ctx.line_to(x, MONTH_HEIGHT);
            ctx.stroke();
        }
        // each month is named once, when its first day or the first day on screen comes round
        let this_month = date.with_day(1).unwrap_or(date);
        if month_from != Some(this_month) {
            month_from = Some(this_month);
            paint_month(ctx, frame, start, this_month);
        }
        let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
        ctx.set_fill_style_str(if weekend { WEEKEND } else { HEADER });
        ctx.fill_rect(x, MONTH_HEIGHT, vp.day_width, day_row);
        let colour = if offset == today {
            TODAY_TEXT
        } else if weekend {
            "#555"
        } else {
            "#aaa"
        };
        if vp.day_width >= 18.0 {
            ctx.set_text_align("center");
            text(
                ctx,
                colour,
                "12px sans-serif",
                &date.day().to_string(),
                x + vp.day_width / 2.0,
                43.0,
            );
            ctx.set_text_align("left");
        }
        ctx.set_stroke_style_str(GRID);
        ctx.begin_path();
        ctx.move_to(x, MONTH_HEIGHT);
        ctx.line_to(x, frame.height);
        ctx.stroke();
    }
    ctx.set_stroke_style_str(BORDER);
    ctx.begin_path();
    ctx.move_to(0.0, HEADER_HEIGHT);
    ctx.line_to(frame.width, HEADER_HEIGHT);
    ctx.stroke();
}

/// The name of a month, at the left of the month and held at the left of the chart while the month
/// is partly off it, until the end of the month pushes it out
fn paint_month(ctx: &CanvasRenderingContext2d, frame: &Frame, start: NaiveDate, month: NaiveDate) {
    let vp = frame.viewport;
    let next = month
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(month);
    let from = vp.x((month - start).num_days() as f64);
    let to = vp.x((next - start).num_days() as f64);
    let label = format!("{} {}", format::month(month), month.year());
    ctx.set_font("13px sans-serif");
    let width = ctx.measure_text(&label).map_or(0.0, |m| m.width());
    let x = (from + 4.0).max(4.0).min(to - width - 4.0);
    if x + width > 0.0 && x < frame.width && x >= from {
        text(ctx, "#888", "13px sans-serif", &label, x, 15.0);
    }
}

fn paint_group(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    p: &Placed,
    rect: Rect,
    marked: &Marked,
    hits: &mut Hits,
) {
    path_round(ctx, rect, 8.0);
    ctx.set_fill_style_str(GROUP_FILL);
    ctx.fill();
    let outline = border(frame, p.mark, p.target, marked);
    ctx.set_stroke_style_str(outline.as_ref().map_or(GROUP_LINE, |(c, _)| c.as_str()));
    ctx.set_line_width(outline.as_ref().map_or(1.0, |(_, w)| *w));
    ctx.stroke();
    ctx.set_line_width(1.0);

    // the name along the top lane of the group, with the arrow that folds it
    let lane_top = frame.viewport.lane_top(p.lane);
    let strip = Rect {
        x: rect.x,
        y: lane_top,
        w: rect.w,
        h: ROW_HEIGHT,
    };
    let arrow = Rect {
        x: rect.x + 4.0,
        y: lane_top + 4.0,
        w: 20.0,
        h: ROW_HEIGHT - 8.0,
    };
    let label = if p.collapsed {
        format!("▸  {} ({})", p.name, p.members)
    } else {
        format!("▾  {}", p.name)
    };
    ctx.save();
    clip_rect(
        ctx,
        Rect {
            x: rect.x + 4.0,
            y: lane_top,
            w: (rect.w - 8.0).max(0.0),
            h: ROW_HEIGHT,
        },
    );
    ctx.set_text_baseline("middle");
    let colour = if frame.hover == Some(p.mark) {
        "#f5d040"
    } else {
        "#aaa"
    };
    text(
        ctx,
        colour,
        "13px sans-serif",
        &label,
        rect.x + 8.0,
        lane_top + ROW_HEIGHT / 2.0,
    );
    ctx.set_text_baseline("alphabetic");
    ctx.restore();
    hits.headers.push((p.mark, strip, arrow));
}

fn paint_links(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    geometry: &HashMap<Mark, Geometry>,
    group_boxes: &HashMap<Mark, Rect>,
) {
    ctx.set_line_width(1.5);
    let mut diamonds: Vec<(Mark, (f64, f64), &str)> = Vec::new();
    for link in frame.links {
        let (Some(from), Some(to)) = (
            geometry.get(&mark_of(link.from)),
            geometry.get(&mark_of(link.to)),
        ) else {
            continue;
        };
        let from_point = match link.from {
            Anchor::Start(_) | Anchor::PlanStart => from.start,
            Anchor::End(_) => from.end,
        };
        let to_point = match link.to {
            Anchor::Start(_) | Anchor::PlanStart => to.start,
            Anchor::End(_) => to.end,
        };
        let (line, solid) = match link.role {
            Role::Before => (BEFORE_LINE, BEFORE),
            Role::After => (AFTER_LINE, AFTER),
        };
        ctx.set_stroke_style_str(line);
        ctx.begin_path();
        ctx.move_to(from_point.0, from_point.1);
        ctx.line_to(to_point.0, to_point.1);
        ctx.stroke();
        for (anchor, point) in [(link.from, from_point), (link.to, to_point)] {
            if matches!(
                anchor,
                Anchor::Start(PlanNode::TaskGroup(_)) | Anchor::End(PlanNode::TaskGroup(_))
            ) {
                diamonds.push((mark_of(anchor), point, solid));
            }
        }
    }
    // the border of a group lights up from the edge a line meets, fading over a quarter of its width
    for (mark, (x, _), colour) in &diamonds {
        // a group under the pointer is plain blue all round
        let (Some(group), false) = (group_boxes.get(mark), frame.hover == Some(*mark)) else {
            continue;
        };
        let reach = group.w * 0.25;
        let left = (*x - group.x).abs() < (*x - group.x - group.w).abs();
        let inward = if left { *x + reach } else { *x - reach };
        let glow = ctx.create_linear_gradient(*x, 0.0, inward, 0.0);
        let _ = glow.add_color_stop(0.0, colour);
        let _ = glow.add_color_stop(1.0, &faded(colour));
        ctx.set_stroke_style_canvas_gradient(&glow);
        ctx.set_line_width(3.0);
        path_round(ctx, *group, 8.0);
        ctx.stroke();
    }
    // a small milestone marker where a line meets the edge of a group
    for (_, (x, y), colour) in diamonds {
        path_diamond(ctx, x, y, 5.0);
        ctx.set_fill_style_str(colour);
        ctx.fill();
    }
    ctx.set_line_width(1.0);
}

/// `#rrggbb` with no opacity, to fade a gradient out without going through black
fn faded(hex: &str) -> String {
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("rgba({},{},{},0)", channel(1), channel(3), channel(5))
}

fn stroke_border(ctx: &CanvasRenderingContext2d, outline: Option<(String, f64)>) {
    if let Some((colour, width)) = outline {
        ctx.set_stroke_style_str(&colour);
        ctx.set_line_width(width);
        ctx.stroke();
        ctx.set_line_width(1.0);
    }
}

/// The blue wash on the right of something nothing depends on, kept inside the path `shape` draws
fn leaf_wash(
    ctx: &CanvasRenderingContext2d,
    rect: Rect,
    shape: impl Fn(&CanvasRenderingContext2d),
) {
    let width = rect.w.min(28.0);
    let gradient = ctx.create_linear_gradient(rect.x + rect.w - width, 0.0, rect.x + rect.w, 0.0);
    let _ = gradient.add_color_stop(0.0, "rgba(30,136,229,0)");
    let _ = gradient.add_color_stop(1.0, "rgba(30,136,229,0.65)");
    ctx.save();
    shape(ctx);
    ctx.clip();
    ctx.set_fill_style_canvas_gradient(&gradient);
    ctx.fill_rect(rect.x + rect.w - width, rect.y, width, rect.h);
    ctx.restore();
}

fn paint_bar(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    p: &Placed,
    (from, to): (f64, f64),
    marked: &Marked,
    hits: &mut Hits,
) {
    let rect = frame.viewport.bar(from, to, p.lane);
    if p.kind == Kind::Buffer {
        paint_buffer(ctx, frame, p, rect, marked, hits);
        return;
    }
    path_round(ctx, rect, RADIUS);
    ctx.set_fill_style_str(look_colour(p.look));
    ctx.fill();
    if p.leaf {
        leaf_wash(ctx, rect, |c| path_round(c, rect, RADIUS));
    }
    path_round(ctx, rect, RADIUS);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));

    // the name inside the bar
    let colour = if p.look == Look::InProgress {
        "rgba(0,0,0,0.85)"
    } else {
        "rgba(255,255,255,0.9)"
    };
    ctx.save();
    clip_rect(
        ctx,
        Rect {
            x: rect.x + 4.0,
            y: rect.y,
            w: (rect.w - 8.0).max(0.0),
            h: rect.h,
        },
    );
    ctx.set_text_baseline("middle");
    text(
        ctx,
        colour,
        "16px sans-serif",
        &p.name,
        rect.x + 6.0,
        rect.y + rect.h / 2.0,
    );
    ctx.set_text_baseline("alphabetic");
    ctx.restore();
    hits.items.push((p.mark, rect));
    if p.violated {
        paint_violation(
            ctx,
            p.mark,
            rect.x + rect.w + 10.0,
            rect.y + rect.h / 2.0,
            hits,
        );
    }
}

/// A buffer is a dotted line between two dotted end bars, with its days written on it
fn paint_buffer(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    p: &Placed,
    rect: Rect,
    marked: &Marked,
    hits: &mut Hits,
) {
    let middle = rect.y + rect.h / 2.0;
    let reach = rect.h / 2.0;
    let _ = ctx.set_line_dash(&js_sys::Array::of2(&3.0.into(), &3.0.into()));
    ctx.set_stroke_style_str("#8a8a8a");
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.move_to(rect.x, middle);
    ctx.line_to(rect.x + rect.w, middle);
    for x in [rect.x, rect.x + rect.w] {
        ctx.move_to(x, middle - reach);
        ctx.line_to(x, middle + reach);
    }
    ctx.stroke();
    let _ = ctx.set_line_dash(&js_sys::Array::new());
    ctx.set_line_width(1.0);
    // the days sit on the line, which stops for them
    ctx.set_font("13px sans-serif");
    let width = ctx.measure_text(&p.name).map_or(0.0, |m| m.width());
    let label = Rect {
        x: rect.x + (rect.w - width) / 2.0 - 3.0,
        y: middle - 8.0,
        w: width + 6.0,
        h: 16.0,
    };
    if label.w < rect.w {
        ctx.set_fill_style_str(BACKGROUND);
        ctx.fill_rect(label.x, label.y, label.w, label.h);
        ctx.set_text_baseline("middle");
        text(
            ctx,
            "#bbb",
            "13px sans-serif",
            &p.name,
            label.x + 3.0,
            middle,
        );
        ctx.set_text_baseline("alphabetic");
    }
    path_round(ctx, rect, RADIUS);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));
    hits.items.push((p.mark, rect));
}

fn paint_violation(ctx: &CanvasRenderingContext2d, mark: Mark, x: f64, y: f64, hits: &mut Hits) {
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    text(ctx, PROBLEM, "bold 16px sans-serif", "!", x, y);
    ctx.set_text_align("left");
    ctx.set_text_baseline("alphabetic");
    hits.violations.push((
        mark,
        Rect {
            x: x - 8.0,
            y: y - 10.0,
            w: 16.0,
            h: 20.0,
        },
    ));
}

fn paint_milestone(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    p: &Placed,
    from: f64,
    marked: &Marked,
    hits: &mut Hits,
) {
    let (cx, cy) = frame.viewport.start_centre(from, p.lane);
    let r = 10.0;
    path_diamond(ctx, cx, cy, r);
    ctx.set_fill_style_str(milestone_colour(p.look));
    ctx.fill();
    if p.leaf {
        let square = Rect {
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
        };
        leaf_wash(ctx, square, |c| path_diamond(c, cx, cy, r));
    }
    path_diamond(ctx, cx, cy, r);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));
    let name_x = cx + r + 6.0 + if p.violated { 20.0 } else { 0.0 };
    let hovered = frame.hover == Some(p.mark);
    ctx.set_text_baseline("middle");
    text(
        ctx,
        if hovered { "#f5d040" } else { "#bbb" },
        "14px sans-serif",
        &p.name,
        name_x,
        cy,
    );
    ctx.set_text_baseline("alphabetic");
    hits.items.push((
        p.mark,
        Rect {
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
        },
    ));
    if p.violated {
        paint_violation(ctx, p.mark, cx + r + 10.0, cy, hits);
    }
}

fn paint_plan_start(
    ctx: &CanvasRenderingContext2d,
    frame: &Frame,
    p: &Placed,
    from: f64,
    marked: &Marked,
    hits: &mut Hits,
) {
    let (cx, cy) = frame.viewport.start_centre(from, p.lane);
    let r = 9.0;
    path_diamond(ctx, cx, cy, r);
    ctx.set_fill_style_str(if frame.hover == Some(p.mark) {
        "#c4b5fd"
    } else {
        PLAN_START
    });
    ctx.fill();
    path_diamond(ctx, cx, cy, r);
    stroke_border(ctx, border(frame, p.mark, false, marked));
    ctx.set_text_baseline("middle");
    text(
        ctx,
        "#bbb",
        "13px sans-serif",
        "Plan Start",
        cx + r + 6.0,
        cy,
    );
    ctx.set_text_baseline("alphabetic");
    hits.items.push((
        p.mark,
        Rect {
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
        },
    ));
}

/// The panel at the bottom left that says what the item under the pointer is
fn paint_panel(ctx: &CanvasRenderingContext2d, lines: &[String], height: f64) {
    let (pad, margin, title, body) = (10.0, 8.0, 20.0, 15.0);
    ctx.set_font(&format!("{title}px sans-serif"));
    let mut wide = ctx.measure_text(&lines[0]).map_or(0.0, |m| m.width());
    ctx.set_font(&format!("{body}px sans-serif"));
    for line in &lines[1..] {
        wide = wide.max(ctx.measure_text(line).map_or(0.0, |m| m.width()));
    }
    let rows = (lines.len() - 1) as f64;
    let h = pad * 2.0
        + title * 1.25
        + if rows > 0.0 {
            4.0 + rows * (body * 1.4 + 4.0) - 4.0
        } else {
            0.0
        };
    let rect = Rect {
        x: margin,
        y: height - margin - h,
        w: wide + pad * 2.0,
        h,
    };
    ctx.set_fill_style_str("rgba(0,0,0,0.19)");
    path_round(
        ctx,
        Rect {
            x: rect.x + 2.0,
            y: rect.y + 3.0,
            ..rect
        },
        6.0,
    );
    ctx.fill();
    path_round(ctx, rect, 6.0);
    ctx.set_fill_style_str("rgba(30,30,30,0.86)");
    ctx.fill();
    ctx.set_stroke_style_str("#4a4a4a");
    ctx.stroke();
    ctx.set_text_baseline("top");
    text(
        ctx,
        "#d4d4d4",
        &format!("{title}px sans-serif"),
        &lines[0],
        rect.x + pad,
        rect.y + pad,
    );
    for (i, line) in lines[1..].iter().enumerate() {
        let y = rect.y + pad + title * 1.25 + 4.0 + i as f64 * (body * 1.4 + 4.0);
        text(
            ctx,
            "#8a8a8a",
            &format!("{body}px sans-serif"),
            line,
            rect.x + pad,
            y,
        );
    }
    ctx.set_text_baseline("alphabetic");
}

/// A small note by the pointer for something that has gone wrong
fn paint_tip(
    ctx: &CanvasRenderingContext2d,
    lines: &[String],
    (mx, my): (f64, f64),
    width: f64,
    height: f64,
) {
    let (pad, line_h) = (7.0, 17.0);
    ctx.set_font("12px sans-serif");
    let wide = lines
        .iter()
        .map(|l| ctx.measure_text(l).map_or(0.0, |m| m.width()))
        .fold(0.0, f64::max);
    let (w, h) = (
        wide + pad * 2.0,
        lines.len() as f64 * line_h + pad * 2.0 - 3.0,
    );
    let mut x = mx + 14.0;
    if x + w > width - 4.0 {
        x = mx - w - 10.0;
    }
    let y = (my - h / 2.0).clamp(4.0, (height - 4.0 - h).max(4.0));
    path_round(ctx, Rect { x, y, w, h }, 4.0);
    ctx.set_fill_style_str("rgba(18,18,18,0.95)");
    ctx.fill();
    ctx.set_stroke_style_str(PROBLEM);
    ctx.set_line_width(1.5);
    ctx.stroke();
    ctx.set_line_width(1.0);
    ctx.set_text_baseline("top");
    for (i, line) in lines.iter().enumerate() {
        text(
            ctx,
            "#e8a87c",
            "12px sans-serif",
            line,
            x + pad,
            y + pad + i as f64 * line_h,
        );
    }
    ctx.set_text_baseline("alphabetic");
}
