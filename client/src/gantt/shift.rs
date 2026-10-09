//! Editing with the mouse while shift is held: handles appear by the item the pointer is near,
//! a dot on each edge to drag a dependency to another item, one in the middle to move it in or
//! out of a group, and a grip inside the right edge of tasks and buffers to change how long they run.

use leptos::prelude::Callback;
use plinko::types::dependency::PlanNode;
use web_sys::CanvasRenderingContext2d;

use crate::gantt::{arrange::Kind, layout::Rect, links::Link};

/// How close the pointer has to be to an item for its handles to show
pub const REACH: f64 = 22.0;
/// The colours of a dependency dragged from the left edge (what comes before) and the right
const BEFORE: &str = "#fc1ef1";
const AFTER: &str = "#07fcd7";
/// Handles are light grey with a dark grey middle
const LIGHT: &str = "#c4c4c9";
const LIT: &str = "#ececf0";
const DARK: &str = "#3a3a3e";

/// How far from a handle's middle it can be taken: half its width and half its height, a good
/// deal more than what is drawn
fn reach(handle: Handle) -> (f64, f64) {
    match handle {
        Handle::Move => (18.0, 12.0),
        Handle::Resize => (9.0, 14.0),
        Handle::Link { .. } => (12.0, 12.0),
    }
}

fn direction(right: bool) -> &'static str {
    if right { AFTER } else { BEFORE }
}
/// How near the edge of the chart a drag starts scrolling it
pub const EDGE: f64 = 36.0;
const COLOUR: &str = "#1e88e5";

/// What dragging an item's handle does
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    /// A dependency from the item's edge, to the item it is dropped on
    Link { right: bool },
    /// Moves the item into the group it is dropped on
    Move,
    /// Changes how many days it runs
    Resize,
}

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
    /// Gives the item `days` more days, or fewer when negative
    pub resize: Callback<(PlanNode, i32)>,
    /// The days an item runs for
    pub duration: Callback<PlanNode, u32>,
    /// The links along the longest chain between two items
    pub chain: Callback<(PlanNode, PlanNode), Vec<Link>>,
}

/// The handles an item has, with where each is
pub fn handles(kind: Kind, rect: Rect) -> Vec<(Handle, (f64, f64))> {
    let middle = rect.y + rect.h / 2.0;
    let mut out = Vec::new();
    if matches!(kind, Kind::Task | Kind::Buffer) {
        out.push((Handle::Resize, (rect.x + rect.w - 6.0, middle)));
    }
    out.push((
        Handle::Link { right: true },
        (rect.x + rect.w + 12.0, middle),
    ));
    out.push((Handle::Link { right: false }, (rect.x - 12.0, middle)));
    // a short bar has no room in the middle, so the handle sits just above it
    let up = if rect.w < 60.0 { rect.y - 4.0 } else { middle };
    out.push((Handle::Move, (rect.x + rect.w / 2.0, up)));
    out
}

/// The handle at a point, if there is one
pub fn grabbed(handles: &[(Handle, (f64, f64))], x: f64, y: f64) -> Option<Handle> {
    handles
        .iter()
        .find(|(h, (hx, hy))| {
            let (w, t) = reach(*h);
            (hx - x).abs() <= w && (hy - y).abs() <= t
        })
        .map(|(h, _)| *h)
}

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

/// What is drawn on top of the chart for the handles and the drag in progress
pub struct Overlay {
    pub handles: Vec<(Handle, (f64, f64))>,
    pub pointer: (f64, f64),
    pub drag: Option<DragView>,
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
    /// A ghost of the item with the length it would have, its outline left as it was
    Resize {
        label: String,
        origin: Rect,
        width: f64,
    },
}

pub fn paint(ctx: &CanvasRenderingContext2d, overlay: &Overlay) {
    ctx.save();
    let dragging = overlay.drag.is_some();
    for (handle, (x, y)) in &overlay.handles {
        // only the grabbed one stays while dragging, drawn where the pointer is
        let lit = grabbed(&overlay.handles, overlay.pointer.0, overlay.pointer.1) == Some(*handle);
        if dragging {
            continue;
        }
        ctx.set_line_width(2.0);
        ctx.begin_path();
        match handle {
            Handle::Resize => slot(ctx, *x, *y, lit),
            Handle::Move => grip(ctx, *x, *y, lit),
            Handle::Link { right } => {
                // a dot is grey until it is taken, then it takes the colour of its direction
                ctx.set_fill_style_str(DARK);
                ctx.set_stroke_style_str(if lit { direction(*right) } else { LIGHT });
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
        Some(DragView::Resize {
            label,
            origin,
            width,
        }) => {
            leave_outline(ctx, *origin);
            let grown = Rect {
                w: *width,
                ..*origin
            };
            ghost(ctx, grown, "");
            ctx.set_font("bold 13px sans-serif");
            ctx.set_text_align("center");
            ctx.set_fill_style_str("#fff");
            let _ = ctx.fill_text(label, grown.x + grown.w / 2.0, grown.y - 6.0);
        }
        None => {}
    }
    ctx.restore();
}

/// The dashed outline left where an item was while a ghost of it is dragged
fn leave_outline(ctx: &CanvasRenderingContext2d, r: Rect) {
    ctx.set_stroke_style_str(COLOUR);
    ctx.set_line_width(1.5);
    let _ = ctx.set_line_dash(&js_sys::Array::of2(&4.0.into(), &3.0.into()));
    ctx.stroke_rect(r.x, r.y, r.w, r.h);
    let _ = ctx.set_line_dash(&js_sys::Array::new());
}

/// A see-through copy of an item
fn ghost(ctx: &CanvasRenderingContext2d, r: Rect, name: &str) {
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

/// The move handle: three dark lines on a light plate
fn grip(ctx: &CanvasRenderingContext2d, x: f64, y: f64, lit: bool) {
    ctx.set_fill_style_str(if lit { LIT } else { LIGHT });
    ctx.fill_rect(x - 9.0, y - 7.0, 18.0, 14.0);
    ctx.set_stroke_style_str(DARK);
    ctx.set_line_width(2.0);
    ctx.begin_path();
    for dy in [-3.5, 0.0, 3.5] {
        ctx.move_to(x - 5.0, y + dy);
        ctx.line_to(x + 5.0, y + dy);
    }
    ctx.stroke();
}

/// The resize handle: a dark slot in a light plate
fn slot(ctx: &CanvasRenderingContext2d, x: f64, y: f64, lit: bool) {
    ctx.set_fill_style_str(if lit { LIT } else { LIGHT });
    ctx.fill_rect(x - 4.5, y - 9.0, 9.0, 18.0);
    ctx.set_fill_style_str(DARK);
    ctx.fill_rect(x - 1.5, y - 6.0, 3.0, 12.0);
}

fn dot(ctx: &CanvasRenderingContext2d, (x, y): (f64, f64), colour: &str) {
    ctx.set_fill_style_str(colour);
    ctx.begin_path();
    let _ = ctx.arc(x, y, 6.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
}

fn highlight(ctx: &CanvasRenderingContext2d, over: Option<Rect>, colour: &str) {
    if let Some(r) = over {
        ctx.set_stroke_style_str(colour);
        ctx.set_line_width(3.0);
        ctx.stroke_rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0);
    }
}

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
            handles(kind, rect())
                .iter()
                .any(|(h, _)| *h == Handle::Resize)
        };
        assert!(has(Kind::Task) && has(Kind::Buffer));
        assert!(!has(Kind::Milestone) && !has(Kind::Group));
    }

    #[test]
    fn a_handle_is_taken_close_by_and_the_resize_grip_wins_in_the_corner() {
        let all = handles(Kind::Task, rect());
        let (_, (x, y)) = all[0];
        assert_eq!(grabbed(&all, x + 2.0, y), Some(Handle::Resize));
        assert_eq!(grabbed(&all, 0.0, 0.0), None);
        let edge = all
            .iter()
            .find(|(h, _)| *h == Handle::Link { right: true })
            .unwrap()
            .1;
        assert_eq!(
            grabbed(&all, edge.0, edge.1),
            Some(Handle::Link { right: true })
        );
    }

    #[test]
    fn handles_show_only_near_an_item() {
        assert!(near(rect(), 90.0, 60.0));
        assert!(!near(rect(), 40.0, 60.0));
    }
}
