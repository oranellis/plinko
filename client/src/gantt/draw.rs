//! Painting the chart on a canvas, the way Plinko 1 did: dark, rounded bars with their names on
//! them, diamonds for milestones, groups as faint rounded boxes around their members, and the
//! dependencies of the item under the pointer as lines behind the bars. It draws from what it is
//! given and keeps no state, and it hands back where everything ended up so the pointer can hit it.

// modules {{{
use crate::gantt::pen::Pen;
use chrono::{Datelike, NaiveDate, Weekday};
use plinko::planner::items::dependency::PlanNode;

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
// }}}

// constants {{{
const BACKGROUND: &str = "#1e1e1e";
const HEADER: &str = "#252526";
const WEEKEND: &str = "#1e1e1e";
const BORDER: &str = "#3a3a3c";
const GRID: &str = "#2a2a2c";
const MONTH_LINE: &str = "#4a4a4e";
const TODAY_BAND: &str = "rgba(64,91,200,0.18)";
const TODAY_TEXT: &str = "#4a90d9";
const TARGET: &str = "#ffd600";
const HOVERED: &str = "#1e88e5";
const BEFORE: &str = "#fc1ef1";
const AFTER: &str = "#07fcd7";
const BEFORE_LINE: &str = "rgba(252,30,241,0.66)";
const CHAIN: &str = "#e53935";
const CHAIN_LINE: &str = "rgba(229,57,53,0.85)";
const INSIDE: &str = "#1255a5";
const AFTER_LINE: &str = "rgba(7,252,215,0.66)";
const PROBLEM: &str = "#e65100";
const GROUP_FILL: &str = "rgba(255,255,255,0.035)";
const GROUP_LINE: &str = "#4a4a4c";
const PLAN_FILL: &str = "rgba(74,144,217,0.07)";
const RADIUS: f64 = 6.0;
const BAND: &str = "rgba(255,255,255,0.045)";
const BAND_LINE: &str = "#48484c";
// }}}

// helpers {{{
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
// }}}

/// Everything one frame needs
pub struct Frame<'a> {
    pub width: f64,
    pub height: f64,
    /// Rows above this are out of sight, for the header over them
    pub top: f64,
    pub viewport: Viewport,
    pub scene: &'a Scene,
    pub today: NaiveDate,
    pub hover: Option<Mark>,
    pub selected: Option<Mark>,
    /// What an item being edited waits for and what waits for it, which are outlined in place of
    /// what the links say
    pub staged: Option<&'a super::Staged>,
    /// A task whose bar is drawn without its name, to leave its handles clear
    pub nameless: Option<Mark>,
    /// The item being edited, which is drawn faintly: it is where it was, not where it will be
    pub ghosted: Option<Mark>,
    pub links: &'a [Link],
    /// Something to outline in red, and how strongly
    pub flash: Option<(Mark, f64, &'static str)>,
    /// A short note by the pointer for a violation under it, and where the pointer is
    pub tip: Option<(&'a [String], (f64, f64))>,
}

// Frame supporting types {{{
/// Where the things drawn are, for hit testing
#[derive(Default)]
pub struct Hits {
    items: Vec<(Mark, Rect)>,
    violations: Vec<(Mark, Rect)>,
    /// A group's name strip, and the small arrow in it that folds the group
    headers: Vec<(Mark, Rect, Rect)>,
    /// How far the layers they were found on are from the screen now
    pub offset: (f64, f64),
}

/// Text that is drawn on top of the layers, since what it looks like depends on the pointer
pub struct Label {
    pub mark: Mark,
    pub words: String,
    pub x: f64,
    pub y: f64,
    pub font: &'static str,
    pub colour: &'static str,
    /// The colour while the item is under the pointer
    pub hovered: &'static str,
    pub clip: Option<Rect>,
}

/// What drawing the layers found: where things are, and the text to put on them
#[derive(Default)]
pub struct Found {
    pub hits: Hits,
    pub labels: Vec<Label>,
}

/// Where an item or group sits: its first day, its last day, and for a group its box
#[derive(Clone, Copy)]
struct Geometry {
    start: (f64, f64),
    end: (f64, f64),
}

/// The border an item gets for what it is to the item under the pointer or for being found
/// What is outlined besides the item under the pointer
struct Marked {
    before: Vec<Mark>,
    after: Vec<Mark>,
    /// Everything inside the group under the pointer
    inside: Vec<Mark>,
    /// The group the item being edited is in
    holds: Option<Mark>,
    /// The ends of the chain from the selected item to the one under the pointer
    chain: Vec<Mark>,
}

/// The shape an item is drawn in, to outline it again when something marks it
enum Shape {
    Round(Rect, f64),
    Diamond(f64, f64, f64),
}
// }}}

// impl Hits {{{
impl Hits {
    fn shifted(&self, r: Rect) -> Rect {
        Rect {
            x: r.x + self.offset.0,
            y: r.y + self.offset.1,
            ..r
        }
    }

    /// What is under a point of the screen: warnings first, then items, then the names of groups,
    /// and whether it is on the arrow of a group's name
    pub fn hit(&self, x: f64, y: f64) -> Option<(Mark, bool)> {
        let (x, y) = (x - self.offset.0, y - self.offset.1);
        let over = |list: &[(Mark, Rect)]| {
            list.iter()
                .rev()
                .find(|(_, r)| r.contains(x, y))
                .map(|(m, _)| *m)
        };
        over(&self.violations)
            .or_else(|| over(&self.items))
            .map(|m| (m, false))
            .or_else(|| {
                self.headers
                    .iter()
                    .rev()
                    .find(|(_, strip, _)| strip.contains(x, y))
                    .map(|(m, _, arrow)| (*m, arrow.contains(x, y)))
            })
    }

    pub fn items(&self) -> impl Iterator<Item = (Mark, Rect)> + '_ {
        self.items.iter().map(|(m, r)| (*m, self.shifted(*r)))
    }

    pub fn violations(&self) -> impl Iterator<Item = (Mark, Rect)> + '_ {
        self.violations.iter().map(|(m, r)| (*m, self.shifted(*r)))
    }

    /// Each group with its name strip and the arrow that folds it
    pub fn headers(&self) -> impl Iterator<Item = (Mark, Rect, Rect)> + '_ {
        self.headers
            .iter()
            .map(|(m, strip, arrow)| (*m, self.shifted(*strip), self.shifted(*arrow)))
    }
}
// }}}

// helpers {{{
fn path_round(ctx: &Pen, r: Rect, radius: f64) {
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

fn path_diamond(ctx: &Pen, cx: f64, cy: f64, r: f64) {
    ctx.begin_path();
    ctx.move_to(cx, cy - r);
    ctx.line_to(cx + r, cy);
    ctx.line_to(cx, cy + r);
    ctx.line_to(cx - r, cy);
    ctx.close_path();
}

fn text(ctx: &Pen, colour: &str, font: &str, words: &str, x: f64, y: f64) {
    ctx.set_fill_style_str(colour);
    ctx.set_font(font);
    let _ = ctx.fill_text(words, x, y);
}

fn clip_rect(ctx: &Pen, r: Rect) {
    ctx.begin_path();
    ctx.rect(r.x, r.y, r.w, r.h);
    ctx.clip();
}
// }}}

// impl Marked {{{
impl Marked {
    fn of(frame: &Frame) -> Self {
        let linked = |role: Role| frame.links.iter().filter(move |l| l.role == role);
        let placed = &frame.scene.arrangement.placed;
        let group = placed
            .iter()
            .find(|p| p.kind == Kind::Group && Some(p.mark) == frame.hover);
        // a group's start is only marked by its diamond, the group itself is not before, and
        // the end of a group likewise
        let before = linked(Role::Before)
            .filter(|l| !matches!(l.from, Anchor::Start(PlanNode::TaskGroup(_))))
            .map(|l| mark_of(l.from));
        let after = linked(Role::After)
            .filter(|l| !matches!(l.to, Anchor::End(PlanNode::TaskGroup(_))))
            .map(|l| mark_of(l.to));
        let (before, after) = match frame.staged {
            Some(staged) => (staged.before.clone(), staged.after.clone()),
            None => (before.collect(), after.collect()),
        };
        Self {
            holds: frame.staged.and_then(|s| s.group),
            before,
            after,
            chain: linked(Role::Chain)
                .flat_map(|l| [mark_of(l.from), mark_of(l.to)])
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
// }}}

// border {{{
/// The outline of an item: its colour, how wide it is, and whether it has a glow in the colour
/// of a target round it, which a target has when something else is its outline
fn border(frame: &Frame, mark: Mark, target: bool, marked: &Marked) -> Option<(String, f64, bool)> {
    match frame.flash {
        Some((flashed, alpha, rgb)) if flashed == mark => {
            return Some((format!("rgba({rgb},{alpha})"), 4.0, false));
        }
        _ => {}
    }
    let (colour, width) = if marked.chain.contains(&mark) {
        (CHAIN, 3.5)
    } else if frame.hover == Some(mark) || frame.selected == Some(mark) {
        (HOVERED, 3.5)
    } else if marked.inside.contains(&mark) || marked.holds == Some(mark) {
        (INSIDE, 3.0)
    } else if marked.before.contains(&mark) {
        (BEFORE, 3.0)
    } else if marked.after.contains(&mark) {
        (AFTER, 3.0)
    } else if target {
        return Some((TARGET.to_string(), 3.0, false));
    } else {
        return None;
    };
    Some((colour.to_string(), width, target))
}
// }}}

// helpers {{{
fn mark_of(anchor: Anchor) -> Mark {
    match anchor {
        Anchor::Start(node) | Anchor::End(node) => node,
    }
}

/// Whether an item is on the layer being drawn, with room for the names that run past its end
fn visible(frame: &Frame, p: &Placed) -> bool {
    let vp = frame.viewport;
    let top = vp.lane_top(p.lane);
    if top + ROW_HEIGHT * p.lanes as f64 <= frame.top || top >= frame.height {
        return false;
    }
    // names of milestones and notes on warnings run on past the end of the item
    let long = p.violated || matches!(p.kind, Kind::Milestone);
    let reach = if long { 600.0 } else { 40.0 };
    let start = frame.scene.origin;
    let offset = |date: NaiveDate| (date - start).num_days() as f64;
    let (from, to) = (vp.x(offset(p.span.0)), vp.x(offset(p.span.1)));
    to + reach > 0.0 && from < frame.width + 40.0
}
// }}}

// paint_back {{{
/// The layer behind the items: the colour, today, the lines between days and the boxes of groups.
/// It looks the same whatever the pointer does, so it is drawn once and reused.
pub fn paint_back(ctx: &Pen, frame: &Frame, found: &mut Found) {
    ctx.set_fill_style_str(BACKGROUND);
    ctx.fill_rect(0.0, 0.0, frame.width, frame.height);
    ctx.set_line_width(1.0);
    let start = frame.scene.origin;
    let vp = frame.viewport;
    let today_x = vp.x((frame.today - start).num_days() as f64);
    if today_x + vp.day_width > 0.0 && today_x < frame.width {
        ctx.set_fill_style_str(TODAY_BAND);
        ctx.fill_rect(today_x, 0.0, vp.day_width, frame.height);
    }
    // the lines between days, the ones between months on their own so they are one path each
    let (first, last) = vp.visible_days(frame.width);
    let (mut days, mut months) = (Vec::new(), Vec::new());
    for offset in first..=last {
        let Some(date) = start.checked_add_signed(chrono::Duration::days(offset)) else {
            continue;
        };
        let x = vp.x(offset as f64);
        if date.day() == 1 {
            &mut months
        } else {
            &mut days
        }
        .push(x);
    }
    for (xs, colour) in [(days, GRID), (months, MONTH_LINE)] {
        ctx.set_stroke_style_str(colour);
        ctx.begin_path();
        for x in xs {
            ctx.move_to(x, 0.0);
            ctx.line_to(x, frame.height);
        }
        ctx.stroke();
    }
    paint_bands(ctx, frame);
    let marked = Marked::of(frame);
    for p in &frame.scene.arrangement.placed {
        if p.kind == Kind::Group && visible(frame, p) {
            let rect = group_box(frame, p);
            paint_group(ctx, frame, p, rect, &marked, found);
        }
    }
}
// }}}

// paint_bands {{{
fn paint_bands(ctx: &Pen, frame: &Frame) {
    let vp = frame.viewport;
    let start = frame.scene.origin;
    let (first, last) = vp.visible_days(frame.width);
    for section in &frame.scene.sections {
        let top = vp.lane_top(section.lane);
        let bottom = top + ROW_HEIGHT * section.lanes as f64;
        if bottom <= frame.top || top >= frame.height {
            continue;
        }
        ctx.set_fill_style_str(BAND);
        ctx.fill_rect(0.0, top, frame.width, ROW_HEIGHT);
        ctx.set_stroke_style_str(BAND_LINE);
        ctx.begin_path();
        ctx.move_to(0.0, top + 0.5);
        ctx.line_to(frame.width, top + 0.5);
        ctx.stroke();
        let skip = (section.from - start).num_days();
        let radius = (vp.day_width.min(ROW_HEIGHT) / 2.0 - 4.0).min(11.0);
        if radius >= 3.0 {
            for offset in first.max(skip)..=last {
                let Some((booked, capacity)) = section.days.get((offset - skip) as usize) else {
                    continue;
                };
                if *booked == 0 {
                    continue;
                }
                let cx = vp.x(offset as f64) + vp.day_width / 2.0;
                paint_pie(
                    ctx,
                    (cx, top + ROW_HEIGHT / 2.0),
                    radius,
                    *booked,
                    *capacity,
                );
            }
        }
        if let Some((lane, runs)) = &section.other {
            for (from, to) in runs {
                let offset = |d: &NaiveDate| (*d - start).num_days() as f64;
                let rect = vp.bar(offset(from), offset(to), *lane);
                if rect.x + rect.w < 0.0 || rect.x > frame.width {
                    continue;
                }
                paint_hatched(ctx, rect);
            }
        }
    }
}

/// Green when booked for exactly their hours, orange under, red over; hours past eight go round a ring
fn paint_pie(ctx: &Pen, (cx, cy): (f64, f64), radius: f64, booked: u32, capacity: u8) {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let colour = match booked.cmp(&u32::from(capacity)) {
        std::cmp::Ordering::Greater => "#e57373",
        std::cmp::Ordering::Equal => "#43c463",
        std::cmp::Ordering::Less => "#c98a3c",
    };
    let over = booked.saturating_sub(8).min(8);
    let inner = if over > 0 { radius * 0.78 } else { radius };
    // one sector for what is booked over a disc for the day, so no seams show between hours
    ctx.begin_path();
    let _ = ctx.arc(cx, cy, inner, 0.0, TAU);
    ctx.set_fill_style_str("#2f2f31");
    ctx.fill();
    ctx.begin_path();
    ctx.move_to(cx, cy);
    let share = f64::from(booked.min(8)) / 8.0;
    let _ = ctx.arc(cx, cy, inner, -FRAC_PI_2, TAU * share - FRAC_PI_2);
    ctx.close_path();
    ctx.set_fill_style_str(colour);
    ctx.fill();
    if over > 0 {
        ctx.set_line_width(1.5);
        ctx.set_stroke_style_str(colour);
        ctx.begin_path();
        let _ = ctx.arc(
            cx,
            cy,
            radius - 0.75,
            -FRAC_PI_2,
            TAU * f64::from(over) / 8.0 - FRAC_PI_2,
        );
        ctx.stroke();
        ctx.set_line_width(1.0);
    }
}

fn paint_hatched(ctx: &Pen, rect: Rect) {
    ctx.save();
    path_round(ctx, rect, RADIUS);
    ctx.set_fill_style_str("#3a3a3c");
    ctx.fill();
    ctx.clip();
    ctx.set_stroke_style_str("#555");
    ctx.set_line_width(4.0);
    ctx.begin_path();
    let mut x = rect.x - rect.h;
    while x < rect.x + rect.w {
        ctx.move_to(x, rect.y + rect.h);
        ctx.line_to(x + rect.h, rect.y);
        x += 8.0;
    }
    ctx.stroke();
    ctx.restore();
    ctx.set_line_width(1.0);
}
// }}}

// paint_names {{{
/// Held at the left edge, and at the top while the rest of the band is in sight
pub fn paint_names(ctx: &Pen, frame: &Frame) {
    let vp = frame.viewport;
    ctx.set_text_baseline("middle");
    for section in &frame.scene.sections {
        let top = vp.lane_top(section.lane);
        let bottom = top + ROW_HEIGHT * section.lanes as f64;
        let y = top.max(frame.top).min(bottom - ROW_HEIGHT);
        if bottom <= frame.top || y >= frame.height {
            continue;
        }
        ctx.set_font("bold 13px sans-serif");
        let width = ctx.text_width(&section.name);
        let pill = Rect {
            x: 6.0,
            y: y + 6.0,
            w: width + 44.0,
            h: ROW_HEIGHT - 12.0,
        };
        path_round(ctx, pill, pill.h / 2.0);
        ctx.set_fill_style_str("rgba(30,30,32,0.94)");
        ctx.fill();
        ctx.set_stroke_style_str(BAND_LINE);
        ctx.stroke();
        let (cx, cy) = (pill.x + pill.h / 2.0, pill.y + pill.h / 2.0);
        ctx.begin_path();
        let _ = ctx.arc(cx, cy, pill.h / 2.0 - 3.0, 0.0, std::f64::consts::TAU);
        ctx.set_fill_style_str(crate::person::face_of(section.who));
        ctx.fill();
        ctx.set_text_align("center");
        text(
            ctx,
            "#fff",
            "bold 10px sans-serif",
            &crate::person::initials(&section.name),
            cx,
            cy + 0.5,
        );
        ctx.set_text_align("left");
        text(
            ctx,
            "#ddd",
            "bold 13px sans-serif",
            &section.name,
            pill.x + pill.h + 6.0,
            cy + 0.5,
        );
    }
    ctx.set_text_baseline("alphabetic");
}
// }}}

// paint_nodes {{{
/// The layer of the items themselves, again the same whatever the pointer does
pub fn paint_nodes(ctx: &Pen, frame: &Frame, found: &mut Found) {
    let start = frame.scene.origin;
    let offset = |date: NaiveDate| (date - start).num_days() as f64;
    let marked = Marked::of(frame);
    for p in &frame.scene.arrangement.placed {
        if p.kind == Kind::Group || !visible(frame, p) {
            continue;
        }
        let (from, to) = (offset(p.span.0), offset(p.span.1));
        match (p.mark, p.kind) {
            (_, Kind::Milestone) => paint_milestone(ctx, frame, p, from, &marked, found),
            _ => paint_bar(ctx, frame, p, (from, to), &marked, &mut found.hits),
        }
    }
}
// }}}

// impl Shape {{{
impl Shape {
    fn of(frame: &Frame, p: &Placed) -> Self {
        let start = frame.scene.origin;
        let offset = |date: NaiveDate| (date - start).num_days() as f64;
        let vp = frame.viewport;
        match p.kind {
            Kind::Group => Shape::Round(group_box(frame, p), 8.0),
            Kind::Milestone => {
                let (cx, cy) = vp.start_centre(offset(p.span.0), p.lane);
                Shape::Diamond(cx, cy, 10.0)
            }
            Kind::Task | Kind::Buffer => {
                Shape::Round(vp.bar(offset(p.span.0), offset(p.span.1), p.lane), RADIUS)
            }
        }
    }

    fn trace(&self, ctx: &Pen) {
        match *self {
            Shape::Round(rect, radius) => path_round(ctx, rect, radius),
            Shape::Diamond(x, y, r) => path_diamond(ctx, x, y, r),
        }
    }

    /// Whether any of it can be seen on a chart `width` by `height`
    fn shows(&self, width: f64, height: f64) -> bool {
        let (x, y, w, h) = match *self {
            Shape::Round(r, _) => (r.x, r.y, r.w, r.h),
            Shape::Diamond(x, y, r) => (x - r, y - r, 2.0 * r, 2.0 * r),
        };
        x + w > -4.0 && x < width + 4.0 && y + h > -4.0 && y < height + 4.0
    }
}
// }}}

// paint_marks {{{
/// What the pointer, the selection and a flash do to the items: their outlines, and the colour of
/// the names that are drawn over the layers. `by` is how far the layers are from where they were drawn.
pub fn paint_marks(ctx: &Pen, frame: &Frame, labels: &[Label], by: (f64, f64)) {
    let marked = Marked::of(frame);
    let flashed = frame.flash.map(|f| f.0);
    let mut marks: Vec<Mark> = [
        frame.hover,
        frame.selected,
        flashed,
        marked.holds,
        frame.nameless,
    ]
    .into_iter()
    .flatten()
    .collect();
    for list in [&marked.chain, &marked.inside, &marked.before, &marked.after] {
        marks.extend(list.iter().copied());
    }
    let mut seen: Vec<Mark> = Vec::new();
    for mark in marks {
        if seen.contains(&mark) {
            continue;
        }
        seen.push(mark);
        let Some(p) = frame.scene.placed(mark) else {
            continue;
        };
        let shape = Shape::of(frame, p);
        if !shape.shows(frame.width, frame.height) {
            continue;
        }
        // the layer has the name on the bar, which the bar is drawn over again to be rid of
        if frame.nameless == Some(mark) && p.kind == Kind::Task {
            let ghost = frame.ghosted == Some(mark);
            shape.trace(ctx);
            if ghost {
                ctx.set_fill_style_str(BACKGROUND);
                ctx.fill();
                ctx.set_global_alpha(0.4);
            }
            ctx.set_fill_style_str(look_colour(p.look));
            ctx.fill();
            ctx.set_global_alpha(1.0);
        }
        shape.trace(ctx);
        stroke_border(ctx, border(frame, mark, p.target, &marked));
    }
    ctx.save();
    ctx.translate(by.0, by.1).ok();
    for label in labels {
        let hovered = frame.hover == Some(label.mark);
        if let Some(clip) = label.clip {
            ctx.save();
            clip_rect(ctx, clip);
        }
        ctx.set_text_baseline("middle");
        text(
            ctx,
            if hovered { label.hovered } else { label.colour },
            label.font,
            &label.words,
            label.x,
            label.y,
        );
        if label.clip.is_some() {
            ctx.restore();
        }
    }
    ctx.restore();
    ctx.set_text_baseline("alphabetic");
}
// }}}

// paint_notes {{{
/// The panel about the item under the pointer and the note on a warning, over everything
pub fn paint_notes(ctx: &Pen, frame: &Frame) {
    if let Some((lines, pointer)) = frame.tip {
        paint_tip(ctx, lines, pointer, frame.width, frame.height);
    }
}
// }}}

// paint_header {{{
pub fn paint_header(ctx: &Pen, frame: &Frame) {
    let start = frame.scene.origin;
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
        ctx.set_stroke_style_str(if date.day() == 1 { MONTH_LINE } else { GRID });
        ctx.begin_path();
        ctx.move_to(x, if date.day() == 1 { 0.0 } else { MONTH_HEIGHT });
        ctx.line_to(x, HEADER_HEIGHT);
        ctx.stroke();
    }
    ctx.set_stroke_style_str(BORDER);
    ctx.begin_path();
    ctx.move_to(0.0, HEADER_HEIGHT);
    ctx.line_to(frame.width, HEADER_HEIGHT);
    ctx.stroke();
}
// }}}

// paint_month {{{
/// The name of a month, at the left of the month and held at the left of the chart while the month
/// is partly off it, until the end of the month pushes it out
fn paint_month(ctx: &Pen, frame: &Frame, start: NaiveDate, month: NaiveDate) {
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
// }}}

// paint_group {{{
fn paint_group(
    ctx: &Pen,
    frame: &Frame,
    p: &Placed,
    rect: Rect,
    marked: &Marked,
    found: &mut Found,
) {
    path_round(ctx, rect, 8.0);
    let plan = frame.scene.is_plan(p.mark);
    ctx.set_fill_style_str(if plan { PLAN_FILL } else { GROUP_FILL });
    ctx.fill();
    // a group always has a line round it, which an outline takes the place of
    let line = Some((GROUP_LINE.to_string(), 1.0, false));
    stroke_border(ctx, border(frame, p.mark, p.target, marked).or(line));

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
    let plan = frame.scene.is_plan(p.mark);
    found.labels.push(Label {
        mark: p.mark,
        words: label,
        x: rect.x + 8.0,
        y: lane_top + ROW_HEIGHT / 2.0,
        font: if plan {
            "bold 13px sans-serif"
        } else {
            "13px sans-serif"
        },
        colour: if plan { "#ddd" } else { "#aaa" },
        hovered: "#f5d040",
        clip: Some(Rect {
            x: rect.x + 4.0,
            y: lane_top,
            w: (rect.w - 8.0).max(0.0),
            h: ROW_HEIGHT,
        }),
    });
    found.hits.headers.push((p.mark, strip, arrow));
}
// }}}

// group_box {{{
/// The box a group is drawn in, a little wider than its bar and as tall as its lanes
fn group_box(frame: &Frame, p: &Placed) -> Rect {
    let start = frame.scene.origin;
    let offset = |date: NaiveDate| (date - start).num_days() as f64;
    let vp = frame.viewport;
    let bar = vp.bar(offset(p.span.0), offset(p.span.1), p.lane);
    let expand = 4.0 + 4.0 * f64::from(p.levels);
    // the top and bottom of nested groups share the lines of the grid, only the sides step out
    let inset = 2.0;
    Rect {
        x: bar.x - expand,
        y: vp.lane_top(p.lane) + inset,
        w: bar.w + 2.0 * expand,
        h: ROW_HEIGHT * p.lanes as f64 - 2.0 * inset,
    }
}
// }}}

// locate {{{
/// Where a line meets an item, and the box of a group
fn locate(frame: &Frame, mark: Mark) -> Option<(Geometry, Option<Rect>)> {
    let p = frame.scene.placed(mark)?;
    if p.kind == Kind::Group {
        let rect = group_box(frame, p);
        let middle = rect.y + rect.h / 2.0;
        let geometry = Geometry {
            start: (rect.x, middle),
            end: (rect.x + rect.w, middle),
        };
        return Some((geometry, Some(rect)));
    }
    let start = frame.scene.origin;
    let offset = |date: NaiveDate| (date - start).num_days() as f64;
    let vp = frame.viewport;
    let geometry = Geometry {
        start: vp.start_centre(offset(p.span.0), p.lane),
        end: vp.end_centre(offset(p.span.1), p.lane),
    };
    Some((geometry, None))
}
// }}}

// paint_links {{{
pub fn paint_links(ctx: &Pen, frame: &Frame) {
    ctx.set_line_width(1.5);
    let mut diamonds: Vec<(Mark, (f64, f64), &str)> = Vec::new();
    for link in frame.links {
        let (Some((from, _)), Some((to, _))) = (
            locate(frame, mark_of(link.from)),
            locate(frame, mark_of(link.to)),
        ) else {
            continue;
        };
        let from_point = match link.from {
            Anchor::Start(_) => from.start,
            Anchor::End(_) => from.end,
        };
        let to_point = match link.to {
            Anchor::Start(_) => to.start,
            Anchor::End(_) => to.end,
        };
        let (line, solid) = match link.role {
            Role::Before => (BEFORE_LINE, BEFORE),
            Role::After => (AFTER_LINE, AFTER),
            Role::Chain => (CHAIN_LINE, CHAIN),
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
        let (Some(group), false) = (
            locate(frame, *mark).and_then(|g| g.1),
            frame.hover == Some(*mark),
        ) else {
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
        path_round(ctx, group, 8.0);
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
// }}}

// helpers {{{
/// `#rrggbb` with no opacity, to fade a gradient out without going through black
fn faded(hex: &str) -> String {
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("rgba({},{},{},0)", channel(1), channel(3), channel(5))
}

fn stroke_border(ctx: &Pen, outline: Option<(String, f64, bool)>) {
    if let Some((colour, width, glow)) = outline {
        if glow {
            ctx.set_global_alpha(0.55);
            ctx.set_stroke_style_str(TARGET);
            ctx.set_line_width(width + 6.0);
            ctx.stroke();
            ctx.set_global_alpha(1.0);
        }
        ctx.set_stroke_style_str(&colour);
        ctx.set_line_width(width);
        ctx.stroke();
        ctx.set_line_width(1.0);
    }
}
// }}}

// paint_bar {{{
fn paint_bar(
    ctx: &Pen,
    frame: &Frame,
    p: &Placed,
    (from, to): (f64, f64),
    marked: &Marked,
    hits: &mut Hits,
) {
    let rect = frame.viewport.bar(from, to, p.lane);
    if p.kind == Kind::Buffer {
        match p.mark {
            PlanNode::PlanDelay(_) => paint_delay(ctx, frame, p, rect, marked, hits),
            _ => paint_buffer(ctx, frame, p, rect, marked, hits),
        }
        return;
    }
    path_round(ctx, rect, RADIUS);
    ctx.set_fill_style_str(look_colour(p.look));
    ctx.fill();
    path_round(ctx, rect, RADIUS);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));

    // the name inside the bar
    let colour = if p.look == Look::InProgress {
        "rgba(0,0,0,0.85)"
    } else {
        "rgba(255,255,255,0.9)"
    };
    ctx.set_font("16px sans-serif");
    // a name that fits needs no clipping, which is slow to draw
    let clipped = ctx.text_width(&p.name) + 10.0 > rect.w;
    if clipped {
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
    }
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
    if clipped {
        ctx.restore();
    }
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
// }}}

// paint_buffer {{{
/// A buffer is the outline of a bar with nothing in it but its days: dotted until it has a day
/// it is set to start on, then solid
fn paint_buffer(
    ctx: &Pen,
    frame: &Frame,
    p: &Placed,
    rect: Rect,
    marked: &Marked,
    hits: &mut Hits,
) {
    if !p.pinned {
        let _ = ctx.set_line_dash(&js_sys::Array::of2(&3.0.into(), &3.0.into()));
    }
    ctx.set_stroke_style_str("#8a8a8a");
    ctx.set_line_width(1.5);
    path_round(ctx, rect, RADIUS);
    ctx.stroke();
    let _ = ctx.set_line_dash(&js_sys::Array::new());
    ctx.set_line_width(1.0);
    ctx.set_font("13px sans-serif");
    let width = ctx.measure_text(&p.name).map_or(0.0, |m| m.width());
    if width + 6.0 < rect.w {
        ctx.set_text_baseline("middle");
        text(
            ctx,
            "#bbb",
            "13px sans-serif",
            &p.name,
            rect.x + (rect.w - width) / 2.0,
            rect.y + rect.h / 2.0,
        );
        ctx.set_text_baseline("alphabetic");
    }
    path_round(ctx, rect, RADIUS);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));
    hits.items.push((p.mark, rect));
}
// }}}

// paint_delay {{{
/// A delay is a dotted line between two dotted end bars, with its name written on it
fn paint_delay(ctx: &Pen, frame: &Frame, p: &Placed, rect: Rect, marked: &Marked, hits: &mut Hits) {
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
    // the name sits on the line, which stops for it
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
// }}}

// paint_violation {{{
fn paint_violation(ctx: &Pen, mark: Mark, x: f64, y: f64, hits: &mut Hits) {
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
// }}}

// paint_milestone {{{
fn paint_milestone(
    ctx: &Pen,
    frame: &Frame,
    p: &Placed,
    from: f64,
    marked: &Marked,
    found: &mut Found,
) {
    let (cx, cy) = frame.viewport.start_centre(from, p.lane);
    let r = 10.0;
    path_diamond(ctx, cx, cy, r);
    ctx.set_fill_style_str(milestone_colour(p.look));
    ctx.fill();
    path_diamond(ctx, cx, cy, r);
    stroke_border(ctx, border(frame, p.mark, p.target, marked));
    found.labels.push(Label {
        mark: p.mark,
        words: p.name.clone(),
        x: cx + r + 6.0 + if p.violated { 20.0 } else { 0.0 },
        y: cy,
        font: "14px sans-serif",
        colour: "#bbb",
        hovered: "#f5d040",
        clip: None,
    });
    found.hits.items.push((
        p.mark,
        Rect {
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
        },
    ));
    if p.violated {
        paint_violation(ctx, p.mark, cx + r + 10.0, cy, &mut found.hits);
    }
}
// }}}

// paint_tip {{{
/// A small note by the pointer for something that has gone wrong
fn paint_tip(ctx: &Pen, lines: &[String], (mx, my): (f64, f64), width: f64, height: f64) {
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
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_follow_the_layers_to_where_they_are_on_the_screen() {
        let node = PlanNode::Task(uuid::Uuid::from_u128(1));
        let mut hits = Hits::default();
        hits.items.push((
            node,
            Rect {
                x: 100.0,
                y: 100.0,
                w: 50.0,
                h: 20.0,
            },
        ));
        // drawn where the screen was, then the chart scrolled 30 pixels left and 10 up
        assert_eq!(hits.hit(110.0, 110.0), Some((node, false)));
        hits.offset = (-30.0, -10.0);
        assert_eq!(hits.hit(110.0, 115.0), None);
        assert_eq!(hits.hit(80.0, 100.0), Some((node, false)));
        assert_eq!(
            hits.items().next().map(|(_, r)| (r.x, r.y)),
            Some((70.0, 90.0))
        );
    }
}
// }}}
