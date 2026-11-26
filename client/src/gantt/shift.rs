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
const PIN: &str = "#ffb300";
const PIN_BAND: &str = "rgba(255,179,0,0.16)";
// }}}

// helpers {{{
/// How far from a handle's middle it can be taken: half its width and half its height, a good
/// deal more than what is drawn
fn reach(handle: Handle) -> (f64, f64) {
    match handle {
        Handle::Move => (0.0, 0.0),
        Handle::Resize | Handle::Start => (9.0, 14.0),
        Handle::Timer { .. } => (9.0, 10.0),
        Handle::Day => (11.0, 9.0),
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
pub const HOLD_BUTTON: f64 = 28.0;
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
    /// A click holds its first day to a day or lets it go, or its last day for `end`
    Timer { end: bool },
    /// The day it is held to: dragged to another, or clicked to pick one and how it is held
    Day,
    /// A click holds it this way, or for `None` lets it go
    Hold(Option<Held>),
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

/// A pin as it is drawn: the edge of the day that is held, by the item's row, the day's cell in
/// the header and how far down the chart goes
#[derive(Clone, Copy, Debug)]
pub struct PinView {
    pub x: f64,
    pub row: Rect,
    pub cell: Rect,
    pub height: f64,
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
    /// Moves the days the item really ran by `days`
    pub shift_dates: Callback<(PlanNode, i32)>,
    /// The day the item is held to in the plan
    pub pin: Callback<PlanNode, Option<Pin>>,
    /// `(node, end, day)`: holds the first day of the item to a day, or its last for `end`
    pub constrain: Callback<(PlanNode, bool, NaiveDate)>,
    /// `(node, end)`: a click on the timer by an end of the item
    pub timer: Callback<(PlanNode, bool)>,
    /// How the item is held to its day, or with `None` that it no longer is
    pub hold: Callback<(PlanNode, Option<Held>)>,
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
    /// Where the item being edited would run, shown over it when nothing is dragged
    pub ghost: Option<Rect>,
    /// The dot that clicks on items answer to: the right one, or the left
    pub picked: Option<bool>,
    /// The day the item is held to, and whether a click on another day moves it
    pub pin: Option<Pin>,
    pub daying: bool,
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
    // under the handles, which are on it
    if let Some(r) = overlay.ghost.filter(|_| !dragging) {
        ghost(ctx, r, "");
    }
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
            Handle::Timer { end } => {
                let active = overlay.pin.is_some_and(|p| p.end == *end);
                timer(ctx, *x, *y, active, lit);
            }
            Handle::Day => day_handle(ctx, *x, *y, lit || overlay.daying),
            Handle::Hold(held) => {
                let active = held.is_some() && overlay.pin.map(|p| p.held) == *held;
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

/// The edge of a held day as a tick, with arrows that say which way the item may still go: on
/// from it for an earliest day, back from it for a latest, and both at it for a fixed one
fn held_sign(ctx: &Pen, x: f64, y: f64, held: Held, scale: f64) {
    let (tick, near, far, head) = (7.0 * scale, 6.0 * scale, 30.0 * scale, 7.0 * scale);
    ctx.set_line_cap("round");
    ctx.set_line_join("round");
    ctx.begin_path();
    ctx.move_to(x, y - tick);
    ctx.line_to(x, y + tick);
    ctx.stroke();
    match held {
        Held::Earliest => arrow(ctx, x + near, x + far, y, head),
        Held::Latest => arrow(ctx, x - near, x - far, y, head),
        Held::Fixed => {
            arrow(ctx, x + far, x + near, y, head);
            arrow(ctx, x - far, x - near, y, head);
        }
    }
}

/// The day an item is held to: a band down the chart like the one for today, and by the item's
/// row the sign of how it is held, on the edge of the day that is
pub fn paint_pin(ctx: &Pen, pin: &PinView) {
    ctx.save();
    ctx.set_global_alpha(if pin.faint { 0.45 } else { 1.0 });
    ctx.set_fill_style_str(PIN_BAND);
    ctx.fill_rect(pin.cell.x, pin.cell.y, pin.cell.w, pin.height - pin.cell.y);
    ctx.set_stroke_style_str(PIN);
    ctx.set_line_width(2.5);
    held_sign(ctx, pin.x, pin.row.y - 9.0, pin.held, 1.0);
    ctx.restore();
}

/// The timer by an end of the item being edited, lit when that end is held to a day and faint
/// when it is not: a click holds it or lets it go
fn timer(ctx: &Pen, x: f64, y: f64, active: bool, lit: bool) {
    ctx.save();
    ctx.set_global_alpha(if active || lit { 1.0 } else { 0.45 });
    ctx.set_stroke_style_str(if active { PIN } else { LIGHT });
    ctx.set_fill_style_str(DARK);
    ctx.set_line_width(1.8);
    ctx.set_line_cap("round");
    ctx.begin_path();
    let _ = ctx.arc(x, y + 1.0, 6.5, 0.0, std::f64::consts::TAU);
    ctx.fill();
    ctx.stroke();
    ctx.begin_path();
    ctx.move_to(x, y - 3.0);
    ctx.line_to(x, y + 1.0);
    ctx.line_to(x + 3.0, y + 1.0);
    ctx.move_to(x - 2.5, y - 8.0);
    ctx.line_to(x + 2.5, y - 8.0);
    ctx.stroke();
    ctx.restore();
}

/// The handle of the held day, in its cell of the header: dragged to another day, or clicked
/// to pick one and how it is held
fn day_handle(ctx: &Pen, x: f64, y: f64, lit: bool) {
    ctx.set_fill_style_str(PIN);
    ctx.begin_path();
    let _ = ctx.arc(x, y, 5.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
    if lit {
        ctx.set_stroke_style_str(LIT);
        ctx.set_line_width(2.0);
        ctx.stroke();
    }
}

/// A button under the held day for a way to hold it, or to let it go
fn hold_button(ctx: &Pen, x: f64, y: f64, held: Option<Held>, active: bool, lit: bool) {
    let half = HOLD_BUTTON / 2.0 - 2.0;
    ctx.set_fill_style_str("#181818");
    ctx.fill_rect(x - half, y - half, half * 2.0, half * 2.0);
    ctx.set_line_width(1.5);
    ctx.set_stroke_style_str(if active {
        PIN
    } else if lit {
        LIT
    } else {
        "#4a4a50"
    });
    ctx.stroke_rect(x - half, y - half, half * 2.0, half * 2.0);
    ctx.set_stroke_style_str(if active { PIN } else { LIGHT });
    ctx.set_line_width(1.8);
    ctx.set_line_cap("round");
    match held {
        // the sign sits in the middle of the button, which its arrows are to one side of
        Some(Held::Earliest) => held_sign(ctx, x - 6.0, y, Held::Earliest, 0.42),
        Some(Held::Latest) => held_sign(ctx, x + 6.0, y, Held::Latest, 0.42),
        Some(Held::Fixed) => held_sign(ctx, x, y, Held::Fixed, 0.3),
        None => {
            ctx.begin_path();
            ctx.move_to(x - 4.5, y - 4.5);
            ctx.line_to(x + 4.5, y + 4.5);
            ctx.move_to(x + 4.5, y - 4.5);
            ctx.line_to(x - 4.5, y + 4.5);
            ctx.stroke();
        }
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
