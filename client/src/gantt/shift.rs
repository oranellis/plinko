//! Editing with the mouse while shift is held: handles appear by the item the pointer is near,
//! a dot on each edge to drag a dependency to another item, one in the middle to move it in or
//! out of a group, and a grip inside the right edge of tasks and buffers to change how long they run.

// modules {{{
use crate::gantt::pen::Pen;
use chrono::NaiveDate;
use leptos::prelude::Callback;
use plinko::{common::dates::TimeConstraint, planner::items::dependency::PlanNode};

use crate::gantt::{arrange::Kind, layout::Rect, links::Link};
// }}}

// constants {{{
/// How close the pointer has to be to an item for its handles to show
pub const REACH: f64 = 22.0;
/// The colours of a dependency dragged from the left edge (what comes before) and the right
const BEFORE: &str = "#fc1ef1";
const AFTER: &str = "#07fcd7";
/// Handles are light grey with a dark grey middle
const LIGHT: &str = "#c4c4c9";
const LIT: &str = "#ececf0";
const DARK: &str = "#3a3a3e";
/// The colour of the day an item is held to
const PIN: &str = "#c8e63c";
const PIN_BAND: &str = "rgba(200,230,60,0.13)";
const PIN_GONE: &str = "rgba(200,230,60,0)";
/// How far along each edge the mark of an item that is held to a day goes, and how round the
/// corner of an item is
const TAG: f64 = 18.0;
const CORNER: f64 = 6.0;
/// The panel and the lines of the bars of buttons on the page
const PANEL: &str = "#181818";
const LINE: &str = "#3a3a3c";
// }}}

// helpers {{{
/// How far from a handle's middle it can be taken: half its width and half its height, a good
/// deal more than what is drawn
fn reach(handle: Handle) -> (f64, f64) {
    match handle {
        Handle::Move => (0.0, 0.0),
        Handle::Resize | Handle::Start => (9.0, 14.0),
        Handle::Hold(_) => (HOLD_BUTTON / 2.0, HOLD_BUTTON / 2.0),
        Handle::Link { .. } => (12.0, 12.0),
    }
}

fn direction(right: bool) -> &'static str {
    if right { AFTER } else { BEFORE }
}
// }}}
// constants {{{
/// How wide and high a button for how a day is held is, with the room around it
pub const HOLD_BUTTON: f64 = 44.0;
/// How near the edge of the chart a drag starts scrolling it
pub const EDGE: f64 = 36.0;
const COLOUR: &str = "#1e88e5";
// }}}

/// What dragging an item's handle does
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    /// A dependency from the item's edge, to the item it is dropped on
    Link { right: bool },
    /// Moves the item into the group it is dropped on
    Move,
    /// Changes how many days it runs, or the day it really ended
    Resize,
    /// Changes the day it really started
    Start,
    /// A click holds it this way
    Hold(Held),
}

// Handle supporting types {{{
/// A handle being dragged, and what it was grabbed from
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Link {
        node: PlanNode,
        right: bool,
    },
    Move(PlanNode),
    Resize {
        node: PlanNode,
        /// The day offset of the last day it runs
        last_day: i64,
        duration: u32,
    },
    Start {
        node: PlanNode,
        /// The day offset of the first day it ran
        first_day: i64,
    },
    /// The held day, taken to another
    Flag {
        node: PlanNode,
        end: bool,
    },
    /// A handle that is only clicked
    Press {
        node: PlanNode,
        handle: Handle,
    },
}

/// The day an item is held to: no earlier, no later or exactly, by its first day or its last
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    pub day: NaiveDate,
    pub held: Held,
    pub end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    Earliest,
    Latest,
    Fixed,
}

/// How far the sign of a pin reaches each side of the edge it is on
pub fn sign_reach(held: Held) -> (f64, f64) {
    match held {
        Held::Earliest => (4.0, 34.0),
        Held::Latest => (34.0, 4.0),
        Held::Fixed => (14.0, 14.0),
    }
}

/// A pin as it is drawn: the edge of the day that is held, by the item's row, the day's cell in
/// the header and how far down the chart goes
#[derive(Clone, Copy, Debug)]
pub struct PinView {
    pub x: f64,
    pub row: Rect,
    pub cell: Rect,
    pub height: f64,
    /// How far down its sign is, which is put where nothing else is
    pub sign_y: f64,
    pub held: Held,
    /// For an item that is only looked at, not edited
    pub faint: bool,
}

/// What the chart asks of the plan, which the page answers
#[derive(Clone, Copy)]
pub struct Editing {
    /// `(node, other, right)`: the other depends on the node for the right edge, else the node on it
    pub link: Callback<(PlanNode, PlanNode, bool)>,
    /// `(selected, other)`: drops the dependency between them, whichever way it goes
    pub unlink: Callback<(PlanNode, PlanNode)>,
    /// Moves the item into the group of the item it lands on, or out to the top for `None`
    pub moved: Callback<(PlanNode, Option<PlanNode>)>,
    /// Whether the item has started, so dragging it sideways changes the days it really ran
    pub dated: Callback<PlanNode, bool>,
    /// Whether the days it really ran have an end, which its right grip then moves
    pub ended: Callback<PlanNode, bool>,
    /// Moves the days the item really ran by `days`
    pub shift_dates: Callback<(PlanNode, i32)>,
    /// The day the item is held to in the plan
    pub pin: Callback<PlanNode, Option<Pin>>,
    /// `(node, end, day)`: holds the first day of the item to a day, or its last for `end`
    pub constrain: Callback<(PlanNode, bool, NaiveDate)>,
    /// How the item is held to its day
    pub hold: Callback<(PlanNode, Held)>,
    /// A click on the held day, after which a click on another day moves it there
    pub daying: Callback<PlanNode>,
    /// Moves the day the item really started by `days`
    pub restart: Callback<(PlanNode, i32)>,
    /// Gives the item `days` more days, or fewer when negative
    pub resize: Callback<(PlanNode, i32)>,
    /// The days an item runs for
    pub duration: Callback<PlanNode, u32>,
    /// `(node, right)`: a click on the dot off an edge of the item, with no drag
    pub pick: Callback<(PlanNode, bool)>,
    /// The links along the longest chain between two items
    pub chain: Callback<(PlanNode, PlanNode), Vec<Link>>,
}

/// The handles an item has, with where each is, and the item they belong to
#[derive(Clone, Debug, Default)]
pub struct Grips {
    rect: Rect,
    pub list: Vec<(Handle, (f64, f64))>,
}

/// What is drawn on top of the chart for the handles and the drag in progress
pub struct Overlay {
    pub handles: Grips,
    pub pointer: (f64, f64),
    pub drag: Option<DragView>,
    /// What to say above an item, when nothing is dragged
    pub note: Option<(Rect, String)>,
    /// The dot that clicks on items answer to: the right one, or the left
    pub picked: Option<bool>,
    /// The day the item is held to
    pub pin: Option<Pin>,
}

pub enum DragView {
    /// A line from the edge grabbed, and the item it would land on
    Link {
        from: (f64, f64),
        over: Option<Rect>,
        right: bool,
    },
    /// A ghost of the item following the pointer, its outline left where it was, and the group or
    /// item it would land in
    Move {
        over: Option<Rect>,
        origin: Rect,
        by: (f64, f64),
        name: String,
    },
    /// The pin where it would be dropped
    Flag(PinView),
    /// A ghost of the item where it would run, its outline left as it was
    Resize {
        label: String,
        origin: Rect,
        grown: Rect,
    },
}
// }}}

// impl Grips {{{
impl Pin {
    pub fn of(constraint: TimeConstraint, end: bool) -> Option<Self> {
        let (day, held) = match constraint {
            TimeConstraint::None => return None,
            TimeConstraint::Earliest(day) => (day, Held::Earliest),
            TimeConstraint::Latest(day) => (day, Held::Latest),
            TimeConstraint::Fixed(day) => (day, Held::Fixed),
        };
        Some(Self { day, held, end })
    }

    pub fn constraint(&self) -> TimeConstraint {
        match self.held {
            Held::Earliest => TimeConstraint::Earliest(self.day),
            Held::Latest => TimeConstraint::Latest(self.day),
            Held::Fixed => TimeConstraint::Fixed(self.day),
        }
    }
}

impl Grips {
    /// `started` is for what has really started, which has a grip for that day in its left edge
    pub fn of(kind: Kind, rect: Rect, started: bool) -> Self {
        let middle = rect.y + rect.h / 2.0;
        let mut list = Vec::new();
        if started {
            list.push((Handle::Start, (rect.x + 6.0, middle)));
        }
        if matches!(kind, Kind::Task | Kind::Buffer) {
            list.push((Handle::Resize, (rect.x + rect.w - 6.0, middle)));
        }
        list.push((
            Handle::Link { right: true },
            (rect.x + rect.w + 12.0, middle),
        ));
        list.push((Handle::Link { right: false }, (rect.x - 12.0, middle)));
        list.push((Handle::Move, (rect.x + rect.w / 2.0, middle)));
        Self { rect, list }
    }

    /// The grips for the days a task really ran, with no others: one in its left edge for the
    /// day it started, and one in its right for the day it ended
    pub fn dates(rect: Rect, started: bool, ended: bool) -> Self {
        let middle = rect.y + rect.h / 2.0;
        let start = started.then_some((Handle::Start, (rect.x + 6.0, middle)));
        let end = ended.then_some((Handle::Resize, (rect.x + rect.w - 6.0, middle)));
        Self {
            rect: Rect::default(),
            list: start.into_iter().chain(end).collect(),
        }
    }

    /// The handle at a point, if there is one. The duration grip and the dots come first, and the
    /// move handle is everywhere else on the item.
    pub fn at(&self, x: f64, y: f64) -> Option<Handle> {
        self.list
            .iter()
            .filter(|(h, _)| *h != Handle::Move)
            .find(|(h, (hx, hy))| {
                let (w, t) = reach(*h);
                (hx - x).abs() <= w && (hy - y).abs() <= t
            })
            .map(|(h, _)| *h)
            .or_else(|| (!self.list.is_empty() && self.rect.contains(x, y)).then_some(Handle::Move))
    }
}
// }}}

// helpers {{{
/// Whether a point is near enough to a rectangle for its handles to show
pub fn near(rect: Rect, x: f64, y: f64) -> bool {
    Rect {
        x: rect.x - REACH,
        y: rect.y - REACH / 2.0,
        w: rect.w + 2.0 * REACH,
        h: rect.h + REACH,
    }
    .contains(x, y)
}

/// The change in days and the new duration for dragging the end to the day offset `day`, snapped
/// to the whole day under it. A duration is never under a day.
pub fn resized(day: f64, last_day: i64, duration: u32) -> (i32, u32) {
    let wanted = i64::from(duration) + day.floor() as i64 - last_day;
    let new = wanted.clamp(1, i64::from(u32::MAX)) as u32;
    (new as i32 - duration as i32, new)
}

/// How a resize reads above the bar: `+2 (6d)`
pub fn resize_label(delta: i32, duration: u32) -> String {
    if delta == 0 {
        format!("({duration}d)")
    } else {
        format!("{delta:+} ({duration}d)")
    }
}
// }}}

// paint {{{
pub fn paint(ctx: &Pen, overlay: &Overlay) {
    ctx.save();
    let dragging = overlay.drag.is_some();
    let ways: Vec<(f64, f64)> = overlay
        .handles
        .list
        .iter()
        .filter(|(h, _)| matches!(h, Handle::Hold(_)) && !dragging)
        .map(|(_, at)| *at)
        .collect();
    hold_bar(ctx, &ways);
    for (handle, (x, y)) in &overlay.handles.list {
        // only the grabbed one stays while dragging, drawn where the pointer is
        let lit = overlay.handles.at(overlay.pointer.0, overlay.pointer.1) == Some(*handle);
        if dragging {
            continue;
        }
        ctx.set_line_width(2.0);
        ctx.begin_path();
        match handle {
            Handle::Resize | Handle::Start => slot(ctx, *x, *y, lit),
            Handle::Hold(held) => {
                let active = overlay.pin.map(|p| p.held) == Some(*held);
                hold_button(ctx, *x, *y, *held, active, lit);
            }
            Handle::Move => grip(ctx, *x, *y, lit),
            Handle::Link { right } => {
                // a dot is grey until it is taken, then it takes the colour of its direction
                // and the one that is picked keeps it
                let picked = overlay.picked == Some(*right);
                ctx.set_fill_style_str(DARK);
                ctx.set_stroke_style_str(if lit || picked {
                    direction(*right)
                } else {
                    LIGHT
                });
                ctx.set_line_width(3.0);
                let _ = ctx.arc(*x, *y, 5.5, 0.0, std::f64::consts::TAU);
                ctx.fill();
                ctx.stroke();
            }
        }
    }
    match &overlay.drag {
        Some(DragView::Link { from, over, right }) => {
            let colour = direction(*right);
            highlight(ctx, *over, colour);
            ctx.set_stroke_style_str(colour);
            ctx.set_line_width(2.0);
            ctx.begin_path();
            ctx.move_to(from.0, from.1);
            ctx.line_to(overlay.pointer.0, overlay.pointer.1);
            ctx.stroke();
            dot(ctx, overlay.pointer, colour);
        }
        Some(DragView::Move {
            over,
            origin,
            by,
            name,
        }) => {
            highlight(ctx, *over, COLOUR);
            leave_outline(ctx, *origin);
            ghost(
                ctx,
                Rect {
                    x: origin.x + by.0,
                    y: origin.y + by.1,
                    ..*origin
                },
                name,
            );
        }
        Some(DragView::Flag(pin)) => paint_pin(ctx, pin),
        Some(DragView::Resize {
            label,
            origin,
            grown,
        }) => {
            leave_outline(ctx, *origin);
            ghost(ctx, *grown, "");
            said(ctx, label, grown.x + grown.w / 2.0, grown.y - 6.0);
        }
        None => {
            if let Some((r, note)) = &overlay.note {
                said(ctx, note, r.x + r.w / 2.0, r.y - 6.0);
            }
        }
    }
    ctx.restore();
}
// }}}

// helpers {{{
/// A line with an open head, from `from` to `to` at the height `y`
fn arrow(ctx: &Pen, from: f64, to: f64, y: f64, head: f64) {
    let back = if to > from { -head } else { head };
    ctx.begin_path();
    ctx.move_to(from, y);
    ctx.line_to(to, y);
    ctx.move_to(to + back, y - head * 0.85);
    ctx.line_to(to, y);
    ctx.line_to(to + back, y + head * 0.85);
    ctx.stroke();
}

/// How an item is held to a day. No earlier: an arrow on from a tick. No later: an arrow up to a
/// tick. Exactly: a lock.
fn held_sign(ctx: &Pen, x: f64, y: f64, held: Held, scale: f64) {
    let (tick, near, far, head) = (7.0 * scale, 6.0 * scale, 30.0 * scale, 7.0 * scale);
    ctx.set_line_cap("round");
    ctx.set_line_join("round");
    if held == Held::Fixed {
        // a body with a bow over it that stands clear of it, and a keyhole
        let (wide, top, bow) = (8.0 * scale, y - scale, 5.0 * scale);
        ctx.begin_path();
        ctx.move_to(x - bow, top);
        ctx.line_to(x - bow, top - 4.0 * scale);
        let _ = ctx.arc(x, top - 4.0 * scale, bow, std::f64::consts::PI, 0.0);
        ctx.line_to(x + bow, top);
        ctx.stroke();
        ctx.stroke_rect(x - wide, top, 2.0 * wide, 11.0 * scale);
        ctx.begin_path();
        ctx.move_to(x, top + 4.5 * scale);
        ctx.line_to(x, top + 6.5 * scale);
        ctx.stroke();
        return;
    }
    ctx.begin_path();
    ctx.move_to(x, y - tick);
    ctx.line_to(x, y + tick);
    ctx.stroke();
    match held {
        Held::Earliest => arrow(ctx, x + near, x + far, y, head),
        _ => arrow(ctx, x - far, x - near, y, head),
    }
}

/// The mark of an item that is held to a day: a top corner of its border in the colour of that,
/// the left corner when it is its first day that is held, the right when it is its last
pub fn paint_tag(ctx: &Pen, rect: Rect, end: bool) {
    let (x, into) = match end {
        true => (rect.x + rect.w, -1.0),
        false => (rect.x, 1.0),
    };
    // strongest in the corner, and gone by the ends of it
    let fade = ctx.create_linear_gradient(x, rect.y, x + into * TAG * 0.5, rect.y + TAG * 0.5);
    let _ = fade.add_color_stop(0.0, PIN);
    let _ = fade.add_color_stop(0.35, PIN);
    let _ = fade.add_color_stop(1.0, PIN_GONE);
    ctx.set_stroke_style_canvas_gradient(&fade);
    ctx.set_line_width(3.0);
    ctx.set_line_cap("butt");
    ctx.begin_path();
    ctx.move_to(x, rect.y + TAG.min(rect.h / 2.0));
    let _ = ctx.arc_to(x, rect.y, x + into * CORNER, rect.y, CORNER);
    ctx.line_to(x + into * TAG.min(rect.w / 2.0), rect.y);
    ctx.stroke();
}

/// The day an item is held to: a band down the chart like the one for today, and by the item's
/// row the sign of how it is held, on the edge of the day that is
pub fn paint_pin(ctx: &Pen, pin: &PinView) {
    ctx.save();
    ctx.set_global_alpha(if pin.faint { 0.45 } else { 1.0 });
    ctx.set_fill_style_str(PIN_BAND);
    ctx.fill_rect(pin.cell.x, pin.cell.y, pin.cell.w, pin.height - pin.cell.y);
    // the one being edited has buttons that say how it is held, the others this sign
    if pin.faint {
        ctx.set_stroke_style_str(PIN);
        ctx.set_line_width(2.5);
        held_sign(ctx, pin.x, pin.sign_y, pin.held, 1.0);
    }
    ctx.restore();
}

/// A rectangle with round corners, as a path
fn round(ctx: &Pen, r: Rect, radius: f64) {
    let (x, y, right, bottom) = (r.x, r.y, r.x + r.w, r.y + r.h);
    ctx.begin_path();
    ctx.move_to(x + radius, y);
    let _ = ctx.arc_to(right, y, right, bottom, radius);
    let _ = ctx.arc_to(right, bottom, x, bottom, radius);
    let _ = ctx.arc_to(x, bottom, x, y, radius);
    let _ = ctx.arc_to(x, y, right, y, radius);
    ctx.close_path();
}

/// The bar the ways to hold a day are on, like the bar of buttons under an item
fn hold_bar(ctx: &Pen, buttons: &[(f64, f64)]) {
    let (Some(first), Some(last)) = (buttons.first(), buttons.last()) else {
        return;
    };
    let half = HOLD_BUTTON / 2.0 + 2.0;
    let bar = Rect {
        x: first.0 - half,
        y: first.1 - half,
        w: last.0 - first.0 + 2.0 * half,
        h: 2.0 * half,
    };
    round(ctx, bar, 8.0);
    ctx.set_fill_style_str(PANEL);
    ctx.fill();
    ctx.set_stroke_style_str(LINE);
    ctx.set_line_width(1.0);
    ctx.stroke();
}

/// A button for a way to hold a day, like the buttons of the bar under an item
fn hold_button(ctx: &Pen, x: f64, y: f64, held: Held, active: bool, lit: bool) {
    let half = HOLD_BUTTON / 2.0 - 2.0;
    let colour = match (active, lit) {
        (true, _) => PIN,
        (false, true) => "#d4d4d4",
        (false, false) => "#888",
    };
    let frame = Rect {
        x: x - half,
        y: y - half,
        w: half * 2.0,
        h: half * 2.0,
    };
    round(ctx, frame, 8.0);
    ctx.set_line_width(1.5);
    ctx.set_stroke_style_str(if active || lit { colour } else { LINE });
    ctx.stroke();
    ctx.set_stroke_style_str(colour);
    ctx.set_line_width(2.0);
    ctx.set_line_cap("round");
    match held {
        // the sign sits in the middle of the button, which its arrows are to one side of
        Held::Earliest => held_sign(ctx, x - 9.0, y, held, 0.6),
        Held::Latest => held_sign(ctx, x + 9.0, y, held, 0.6),
        Held::Fixed => held_sign(ctx, x, y + 1.0, held, 0.85),
    }
}

/// Words above an item, centred on `x`
fn said(ctx: &Pen, text: &str, x: f64, y: f64) {
    ctx.set_font("bold 13px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("alphabetic");
    ctx.set_fill_style_str("#fff");
    let _ = ctx.fill_text(text, x, y);
}

/// The dashed outline left where an item was while a ghost of it is dragged
fn leave_outline(ctx: &Pen, r: Rect) {
    ctx.set_stroke_style_str(COLOUR);
    ctx.set_line_width(1.5);
    let _ = ctx.set_line_dash(&js_sys::Array::of2(&4.0.into(), &3.0.into()));
    ctx.stroke_rect(r.x, r.y, r.w, r.h);
    let _ = ctx.set_line_dash(&js_sys::Array::new());
}

/// A see-through copy of an item
fn ghost(ctx: &Pen, r: Rect, name: &str) {
    ctx.set_fill_style_str("rgba(30,136,229,0.45)");
    ctx.fill_rect(r.x, r.y, r.w, r.h);
    ctx.set_stroke_style_str(COLOUR);
    ctx.set_line_width(2.0);
    ctx.stroke_rect(r.x, r.y, r.w, r.h);
    if !name.is_empty() {
        ctx.save();
        ctx.begin_path();
        ctx.rect(r.x, r.y, r.w, r.h);
        ctx.clip();
        ctx.set_font("14px sans-serif");
        ctx.set_text_align("left");
        ctx.set_text_baseline("middle");
        ctx.set_fill_style_str("#fff");
        let _ = ctx.fill_text(name, r.x + 6.0, r.y + r.h / 2.0);
        ctx.restore();
    }
}

/// The move handle: three dark lines
fn grip(ctx: &Pen, x: f64, y: f64, lit: bool) {
    ctx.set_stroke_style_str(if lit { LIT } else { DARK });
    ctx.set_line_width(2.5);
    ctx.set_line_cap("round");
    ctx.begin_path();
    for dy in [-4.0, 0.0, 4.0] {
        ctx.move_to(x - 4.0, y + dy);
        ctx.line_to(x + 4.0, y + dy);
    }
    ctx.stroke();
}

/// The duration handle: one dark vertical line
fn slot(ctx: &Pen, x: f64, y: f64, lit: bool) {
    ctx.set_stroke_style_str(if lit { LIT } else { DARK });
    ctx.set_line_width(3.0);
    ctx.set_line_cap("round");
    ctx.begin_path();
    ctx.move_to(x, y - 7.0);
    ctx.line_to(x, y + 7.0);
    ctx.stroke();
}

fn dot(ctx: &Pen, (x, y): (f64, f64), colour: &str) {
    ctx.set_fill_style_str(colour);
    ctx.begin_path();
    let _ = ctx.arc(x, y, 6.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
}

fn highlight(ctx: &Pen, over: Option<Rect>, colour: &str) {
    if let Some(r) = over {
        ctx.set_stroke_style_str(colour);
        ctx.set_line_width(3.0);
        ctx.stroke_rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0);
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 100.0,
            y: 50.0,
            w: 120.0,
            h: 24.0,
        }
    }

    #[test]
    fn dragging_the_end_snaps_to_days_and_shows_the_change() {
        // four days ending on a friday, the 5th day offset, dragged into the sunday two days on
        assert_eq!(resized(6.4, 4, 4), (2, 6));
        assert_eq!(resized(4.9, 4, 4), (0, 4));
        assert_eq!(resized(2.0, 4, 4), (-2, 2));
        // never under a day
        assert_eq!(resized(-3.0, 4, 4), (-3, 1));
        assert_eq!(resize_label(2, 6), "+2 (6d)");
        assert_eq!(resize_label(-3, 1), "-3 (1d)");
        assert_eq!(resize_label(0, 4), "(4d)");
    }

    #[test]
    fn only_tasks_and_buffers_can_be_resized() {
        let has = |kind| {
            Grips::of(kind, rect(), false)
                .list
                .iter()
                .any(|(h, _)| *h == Handle::Resize)
        };
        assert!(has(Kind::Task) && has(Kind::Buffer));
        assert!(!has(Kind::Milestone) && !has(Kind::Group));
    }

    #[test]
    fn the_whole_item_moves_except_the_right_edge_and_the_dots() {
        let grips = Grips::of(Kind::Task, rect(), false);
        // the item is x 100 to 220, y 50 to 74, the duration grip is inside its right edge
        assert_eq!(grips.at(214.0, 62.0), Some(Handle::Resize));
        assert_eq!(grips.at(110.0, 52.0), Some(Handle::Move));
        assert_eq!(grips.at(160.0, 70.0), Some(Handle::Move));
        assert_eq!(grips.at(232.0, 62.0), Some(Handle::Link { right: true }));
        assert_eq!(grips.at(88.0, 62.0), Some(Handle::Link { right: false }));
        assert_eq!(grips.at(0.0, 0.0), None);
    }

    #[test]
    fn handles_show_only_near_an_item() {
        assert!(near(rect(), 90.0, 60.0));
        assert!(!near(rect(), 40.0, 60.0));
    }
}
// }}}
