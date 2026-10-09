//! The Gantt chart, one canvas as in Plinko 1. Dragging pans it with momentum, the wheel scrolls
//! it, shift and the wheel (or a pinch) zooms, hovering shows an item's dependencies and a panel
//! about it, and tapping opens it.

pub mod arrange;
pub mod draw;
pub mod info;
pub mod layout;
pub mod links;
pub mod scene;
pub mod shift;

use std::{collections::HashMap, time::Duration};

use chrono::NaiveDate;
use leptos::{html, prelude::*};
use plinko::types::dependency::PlanNode;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CanvasRenderingContext2d, PointerEvent, WheelEvent};

use crate::gantt::{
    arrange::Mark,
    draw::{Frame, Hits, paint},
    layout::{Bounds, DAY_WIDTH, Rect, Viewport},
    links::{Link, Role},
    scene::Scene,
    shift::{Drag, DragView, Editing, Handle, Overlay},
};

/// Movement under this many pixels is a tap, not a drag (more for a finger)
const TAP_MOUSE: f64 = 4.0;
const TAP_TOUCH: f64 = 10.0;
/// What is left of a flick each frame
const FRICTION: f64 = 0.85;

#[derive(Default)]
struct Gesture {
    touches: HashMap<i32, (f64, f64)>,
    travelled: f64,
    velocity: (f64, f64),
    last_time: f64,
    touch: bool,
    /// Changes with every new gesture, so a flick left over from an old one stops
    generation: u32,
}

fn now() -> f64 {
    js_sys::Date::now()
}

#[component]
pub fn Gantt(
    scene: Signal<Scene>,
    today: Signal<NaiveDate>,
    /// Where the chart is scrolled, `None` until it has been put on today
    viewport: RwSignal<Option<Viewport>>,
    links: Callback<Mark, Vec<Link>>,
    /// What to say about an item: its name and then its details
    info: Callback<Mark, Vec<String>>,
    /// What went wrong for an item that misses its date
    violation: Callback<Mark, Vec<String>>,
    on_open: Callback<PlanNode>,
    /// What dragging handles with shift held does to the plan
    editing: Editing,
    /// The item selected, whose dependency lines stay on show
    selected: Signal<Option<Mark>>,
    on_toggle: Callback<PlanNode>,
    /// A tap on nothing
    #[prop(optional)]
    on_blank: Option<Callback<()>>,
    /// Counts up each time to jump to today
    jump: RwSignal<u32>,
    /// An item to scroll to and flash
    focus: RwSignal<Option<Mark>>,
    /// An item to put in the middle, with a light outline that fades away
    #[prop(optional)]
    reveal: Option<RwSignal<Option<Mark>>>,
) -> impl IntoView {
    let canvas = NodeRef::<html::Canvas>::new();
    let size = RwSignal::new((800.0, 600.0));
    let hover = RwSignal::new(None::<Mark>);
    let pointer = RwSignal::new((0.0, 0.0));
    let flash = RwSignal::new(None::<(Mark, f64, &'static str)>);
    let hits = StoredValue::new_local(Hits::default());
    let gesture = StoredValue::new_local(Gesture::default());
    // shift editing: whether shift is held, the item whose handles show, and the handle being dragged
    let shift = RwSignal::new(false);
    let nearby = RwSignal::new(None::<PlanNode>);
    let drag = RwSignal::new(None::<Drag>);
    // where a handle was taken, so letting go without moving counts as a click
    let grabbed_at = StoredValue::new_local((0.0, 0.0));

    let bounds = Signal::derive(move || {
        let (width, height) = size.get();
        scene.with(|s| Bounds {
            width,
            height,
            lanes: s.arrangement.lanes,
            last_day: s.last_day,
        })
    });
    let current = move || viewport.get_untracked().unwrap_or_default();
    let today_offset = move || {
        let (start, today) = (
            scene.with_untracked(|s| s.plan_start),
            today.get_untracked(),
        );
        (today - start).num_days() as f64
    };

    // also runs from a timer and a window listener, after the chart may be gone
    let measure = move || {
        if let Some(Some(canvas)) = canvas.try_get_untracked() {
            let element: &web_sys::Element = canvas.as_ref();
            let (w, h) = (
                f64::from(element.client_width()),
                f64::from(element.client_height()),
            );
            if w > 0.0 && h > 0.0 {
                size.try_set((w, h));
            }
        }
    };
    let on_resize = Closure::<dyn FnMut()>::new(measure);
    if let Some(window) = web_sys::window() {
        let _ =
            window.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref());
    }
    on_resize.forget();
    if let Some(window) = web_sys::window() {
        for (event, down) in [("keydown", true), ("keyup", false)] {
            let key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Shift" {
                        shift.try_set(down);
                        if !down {
                            drag.try_set(None);
                        }
                    }
                },
            );
            let _ = window.add_event_listener_with_callback(event, key.as_ref().unchecked_ref());
            key.forget();
        }
        let blur = Closure::<dyn FnMut()>::new(move || {
            shift.try_set(false);
            drag.try_set(None);
        });
        let _ = window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref());
        blur.forget();
    }
    Effect::new(move |_| {
        if let Some(element) = canvas.get() {
            // the chart changes size when the pane beside it opens or closes
            let watch = Closure::<dyn FnMut()>::new(measure);
            if let Ok(observer) = web_sys::ResizeObserver::new(watch.as_ref().unchecked_ref()) {
                observer.observe(element.as_ref());
            }
            watch.forget();
            measure();
            // the page may still be settling into its place
            set_timeout(measure, Duration::from_millis(60));
        }
    });

    // the first time, put today in the middle
    Effect::new(move |_| {
        let (width, _) = size.get();
        if viewport.get_untracked().is_none() && canvas.get().is_some() && width > 0.0 {
            let scroll_x = (today_offset() * DAY_WIDTH - width / 2.0).max(0.0);
            viewport.set(Some(Viewport {
                scroll_x,
                ..Viewport::default()
            }));
        }
    });
    Effect::new(move |previous: Option<u32>| {
        let count = jump.get();
        if previous.is_some() {
            let mut vp = Viewport::default();
            vp.centre_on_day(today_offset(), bounds.get_untracked());
            viewport.set(Some(vp));
        }
        count
    });

    // search: scroll to the item and flash it three times, then let it fade
    Effect::new(move |_| {
        let Some(mark) = focus.get() else {
            return;
        };
        focus.set(None);
        let Some(found) = scene.with_untracked(|s| {
            let placed = s.arrangement.placed.iter().find(|p| p.mark == mark)?;
            Some((
                (placed.span.0 - s.plan_start).num_days() as f64,
                placed.lane,
            ))
        }) else {
            return;
        };
        let mut vp = current();
        let b = bounds.get_untracked();
        vp.centre_on_day(found.0, b);
        vp.centre_on_lane(found.1, b);
        viewport.set(Some(vp));
        flash_item(flash, mark);
    });

    // coming from elsewhere: once the chart has its size, centre the item and fade an outline
    Effect::new(move |_| {
        let Some(mark) = reveal.and_then(|r| r.get()) else {
            return;
        };
        if let Some(r) = reveal {
            r.set(None);
        }
        set_timeout(
            move || {
                let Some(found) = scene.with_untracked(|s| {
                    let placed = s.arrangement.placed.iter().find(|p| p.mark == mark)?;
                    Some((
                        (placed.span.0 - s.plan_start).num_days() as f64,
                        placed.lane,
                    ))
                }) else {
                    return;
                };
                let (Some(current), Some(b)) =
                    (viewport.try_get_untracked(), bounds.try_get_untracked())
                else {
                    return;
                };
                let mut vp = current.unwrap_or_default();
                vp.centre_on_day(found.0, b);
                vp.centre_on_lane(found.1, b);
                viewport.try_set(Some(vp));
                fade_item(flash, mark, LIGHT);
            },
            Duration::from_millis(150),
        );
    });

    // what is under the pointer: warnings first, then items, then the names of groups
    let hit = move |x: f64, y: f64| -> Option<(Mark, bool)> {
        hits.with_value(|h| {
            let over = |list: &[(Mark, layout::Rect)]| {
                list.iter()
                    .rev()
                    .find(|(_, r)| r.contains(x, y))
                    .map(|(m, _)| *m)
            };
            over(&h.violations)
                .map(|m| (m, false))
                .or_else(|| over(&h.items).map(|m| (m, false)))
                .or_else(|| {
                    h.headers
                        .iter()
                        .rev()
                        .find(|(_, strip, _)| strip.contains(x, y))
                        .map(|(m, _, arrow)| (*m, arrow.contains(x, y)))
                })
        })
    };
    // where an item is drawn, a group by its name strip
    let rect_of = move |mark: Mark| -> Option<Rect> {
        hits.with_value(|h| {
            h.items
                .iter()
                .find(|(m, _)| *m == mark)
                .map(|(_, r)| *r)
                .or_else(|| {
                    h.headers
                        .iter()
                        .find(|(m, ..)| *m == mark)
                        .map(|(_, r, _)| *r)
                })
        })
    };
    // the handles of the item the pointer is near
    let handles_of = move |node: PlanNode| {
        let kind = scene.with_untracked(|s| {
            s.arrangement
                .placed
                .iter()
                .find(|p| p.mark == Mark::Node(node))
                .map(|p| p.kind)
        });
        match (kind, rect_of(Mark::Node(node))) {
            (Some(kind), Some(rect)) => shift::handles(kind, rect),
            _ => Vec::new(),
        }
    };

    // draw
    Effect::new(move |_| {
        let (width, height) = size.get();
        let picked = selected.get();
        let (vp, hovered, flashed, today) = (
            viewport.get().unwrap_or_default(),
            hover.get(),
            flash.get(),
            today.get(),
        );
        let at = pointer.get();
        let (editing_shift, near, dragged) = (shift.get(), nearby.get(), drag.get());
        let Some(canvas) = canvas.get() else {
            return;
        };
        let ratio = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
        canvas.set_width((width * ratio) as u32);
        canvas.set_height((height * ratio) as u32);
        let Some(Ok(ctx)) = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .map(|c| c.dyn_into::<CanvasRenderingContext2d>())
        else {
            return;
        };
        let _ = ctx.set_transform(ratio, 0.0, 0.0, ratio, 0.0, 0.0);
        // what is under the pointer shows its lines, and otherwise the selected item does
        // with something selected, the chain to what is under the pointer shows instead
        let chain = match (picked, hovered) {
            (Some(Mark::Node(a)), Some(Mark::Node(b))) if a != b => editing.chain.run((a, b)),
            _ => Vec::new(),
        };
        let linked: Vec<Link> = if chain.is_empty() {
            hovered.or(picked).map(|m| links.run(m)).unwrap_or_default()
        } else {
            chain
        };
        let info: Vec<String> = hovered.map(|m| info.run(m)).unwrap_or_default();
        scene.with(|scene| {
            let over_violation = hovered.filter(|m| {
                hits.with_value(|h| {
                    h.violations
                        .iter()
                        .any(|(v, r)| v == m && r.contains(at.0, at.1))
                })
            });
            let tip = over_violation.map(|m| violation.run(m));
            let frame = Frame {
                width,
                height,
                viewport: vp,
                scene,
                today,
                hover: hovered,
                selected: picked,
                links: &linked,
                flash: flashed,
                info: if over_violation.is_some() { &[] } else { &info },
                tip: tip.as_deref().map(|lines| (lines, at)),
            };
            hits.set_value(paint(&ctx, &frame));
        });
        if editing_shift {
            let handles = near.map(handles_of).unwrap_or_default();
            let target = |own: PlanNode| {
                hit(at.0, at.1)
                    .map(|(m, _)| m)
                    .filter(|m| *m != Mark::Node(own))
                    .and_then(rect_of)
            };
            let view = dragged.map(|d| match d {
                Drag::Link { node, right } => DragView::Link {
                    from: handles
                        .iter()
                        .find(|(h, _)| *h == Handle::Link { right })
                        .map_or(at, |(_, p)| *p),
                    over: target(node),
                },
                Drag::Move(node) => DragView::Move { over: target(node) },
                Drag::Resize {
                    node,
                    last_day,
                    duration,
                } => {
                    let day = viewport.get_untracked().unwrap_or_default().day_at(at.0);
                    let (delta, new) = shift::resized(day, last_day, duration);
                    let above = rect_of(Mark::Node(node)).map_or(at, |r| (at.0, r.y));
                    DragView::Resize {
                        label: shift::resize_label(delta, new),
                        above,
                    }
                }
            });
            shift::paint(
                &ctx,
                &Overlay {
                    handles,
                    pointer: at,
                    drag: view,
                },
            );
        }
    });

    let down = move |ev: PointerEvent| {
        // shift on a handle starts editing instead of panning
        if ev.shift_key() && ev.pointer_type() == "mouse" {
            let (x, y) = (f64::from(ev.offset_x()), f64::from(ev.offset_y()));
            if let Some((node, handle)) = nearby
                .get_untracked()
                .and_then(|node| shift::grabbed(&handles_of(node), x, y).map(|h| (node, h)))
            {
                if let Some(canvas) = canvas.get_untracked() {
                    let element: &web_sys::Element = canvas.as_ref();
                    let _ = element.set_pointer_capture(ev.pointer_id());
                }
                grabbed_at.set_value((x, y));
                drag.set(Some(match handle {
                    Handle::Link { right } => Drag::Link { node, right },
                    Handle::Move => Drag::Move(node),
                    Handle::Resize => {
                        let end = scene.with_untracked(|s| {
                            s.arrangement
                                .placed
                                .iter()
                                .find(|p| p.mark == Mark::Node(node))
                                .map(|p| (s.plan_start, p.span.1))
                        });
                        let last_day = end.map_or(0, |(start, end)| (end - start).num_days() - 1);
                        Drag::Resize {
                            node,
                            last_day,
                            duration: editing.duration.run(node),
                        }
                    }
                }));
                edge_scroll(viewport, bounds, pointer, drag);
                return;
            }
        }
        if let Some(canvas) = canvas.get_untracked() {
            let element: &web_sys::Element = canvas.as_ref();
            let _ = element.set_pointer_capture(ev.pointer_id());
        }
        gesture.update_value(|g| {
            if g.touches.is_empty() {
                g.travelled = 0.0;
                g.velocity = (0.0, 0.0);
                g.touch = ev.pointer_type() != "mouse";
            }
            g.generation += 1;
            g.last_time = now();
            g.touches.insert(
                ev.pointer_id(),
                (f64::from(ev.client_x()), f64::from(ev.client_y())),
            );
        });
    };

    let moved = move |ev: PointerEvent| {
        let (x, y) = (f64::from(ev.offset_x()), f64::from(ev.offset_y()));
        pointer.set((x, y));
        shift.set(ev.shift_key());
        if drag.get_untracked().is_some() {
            return;
        }
        if ev.shift_key() {
            // handles show for the item the pointer is near, and stay while it is on one
            let on_handle = nearby
                .get_untracked()
                .is_some_and(|n| shift::grabbed(&handles_of(n), x, y).is_some());
            if !on_handle {
                let found = hits.with_value(|h| {
                    h.items
                        .iter()
                        .map(|(m, r)| (*m, *r))
                        .chain(h.headers.iter().map(|(m, r, _)| (*m, *r)))
                        .find(|(_, r)| shift::near(*r, x, y))
                        .and_then(|(m, _)| match m {
                            Mark::Node(n) => Some(n),
                            Mark::PlanStart => None,
                        })
                });
                nearby.set(found);
            }
        } else {
            nearby.set(None);
        }
        if ev.pointer_type() == "mouse" {
            let over = hit(x, y).map(|(m, _)| m);
            if hover.get_untracked() != over {
                hover.set(over);
            }
        }
        let here = (f64::from(ev.client_x()), f64::from(ev.client_y()));
        let b = bounds.get_untracked();
        gesture.update_value(|g| {
            let Some(before) = g.touches.get(&ev.pointer_id()).copied() else {
                return;
            };
            let others: Vec<(f64, f64)> = g
                .touches
                .iter()
                .filter(|(id, _)| **id != ev.pointer_id())
                .map(|(_, p)| *p)
                .collect();
            let (dx, dy) = (here.0 - before.0, here.1 - before.1);
            g.travelled += dx.abs() + dy.abs();
            let mut vp = viewport.get_untracked().unwrap_or_default();
            match others.first() {
                None => {
                    vp.drag(dx, dy, b);
                    let dt = now() - g.last_time;
                    g.velocity = if dt > 0.0 {
                        (dx / dt, dy / dt)
                    } else {
                        (0.0, 0.0)
                    };
                }
                Some(other) => {
                    // two fingers: how far apart they are is the zoom
                    let old = (before.0 - other.0).hypot(before.1 - other.1);
                    let new = (here.0 - other.0).hypot(here.1 - other.1);
                    if old > 1.0 {
                        let middle = (here.0 + other.0) / 2.0;
                        let left = canvas.get_untracked().map_or(0.0, |c| {
                            AsRef::<web_sys::Element>::as_ref(&c)
                                .get_bounding_client_rect()
                                .left()
                        });
                        vp.zoom(middle - left, new / old, b);
                    }
                }
            }
            g.last_time = now();
            viewport.set(Some(vp));
            g.touches.insert(ev.pointer_id(), here);
        });
    };

    // a tap on an item selects it, with shift on one linked to the selected item cuts the link
    let tap = move |x: f64, y: f64, shifted: bool| {
        if let Some((Mark::Node(node), fold)) = hit(x, y) {
            let on_violation =
                hits.with_value(|h| h.violations.iter().any(|(_, r)| r.contains(x, y)));
            let chosen = selected.get_untracked();
            match (fold, on_violation) {
                (_, true) => {}
                (true, false) => on_toggle.run(node),
                (false, false) if shifted && chosen.is_some_and(|s| linked_to(links, s, node)) => {
                    if let Some(Mark::Node(own)) = chosen {
                        editing.unlink.run((own, node));
                    }
                }
                (false, false) => on_open.run(node),
            }
        } else if let Some(blank) = on_blank {
            blank.run(());
        }
    };

    let up = move |ev: PointerEvent| {
        let (x, y) = (f64::from(ev.offset_x()), f64::from(ev.offset_y()));
        if let Some(done) = drag.get_untracked() {
            drag.set(None);
            let (from_x, from_y) = grabbed_at.get_value();
            if (from_x - x).hypot(from_y - y) < TAP_MOUSE {
                tap(x, y, ev.shift_key());
                return;
            }
            let over = hit(x, y).and_then(|(m, _)| match m {
                Mark::Node(n) => Some(n),
                Mark::PlanStart => None,
            });
            match done {
                Drag::Link { node, right } => {
                    if let Some(other) = over.filter(|o| *o != node) {
                        editing.link.run((node, other, right));
                    }
                }
                Drag::Move(node) => {
                    if over != Some(node) {
                        editing.moved.run((node, over));
                    }
                }
                Drag::Resize {
                    last_day,
                    duration,
                    node,
                } => {
                    let day = viewport.get_untracked().unwrap_or_default().day_at(x);
                    let (delta, _) = shift::resized(day, last_day, duration);
                    if delta != 0 {
                        editing.resize.run((node, delta));
                    }
                }
            }
            return;
        }
        let (tapped, velocity, generation) = gesture
            .try_update_value(|g| {
                let limit = if g.touch { TAP_TOUCH } else { TAP_MOUSE };
                let tap = g.touches.len() == 1 && g.travelled < limit;
                g.touches.remove(&ev.pointer_id());
                (tap, g.velocity, g.generation)
            })
            .unwrap_or_default();
        if tapped {
            tap(x, y, ev.shift_key());
            return;
        }
        let empty = gesture.with_value(|g| g.touches.is_empty());
        if empty {
            flick(
                viewport,
                bounds,
                gesture,
                generation,
                (velocity.0 * 1000.0, velocity.1 * 1000.0),
            );
        }
    };

    let cancelled = move |ev: PointerEvent| {
        gesture.update_value(|g| {
            g.touches.remove(&ev.pointer_id());
        });
        hover.set(None);
    };

    let wheel = move |ev: WheelEvent| {
        ev.prevent_default();
        let b = bounds.get_untracked();
        let mut vp = viewport.get_untracked().unwrap_or_default();
        let x = f64::from(ev.offset_x());
        if ev.shift_key() {
            vp.zoom(x, if ev.delta_y() > 0.0 { 0.9 } else { 1.1 }, b);
        } else if ev.ctrl_key() || ev.meta_key() {
            vp.zoom(x, (-ev.delta_y() / 300.0).exp(), b);
        } else {
            vp.scroll(ev.delta_x(), ev.delta_y(), b);
        }
        viewport.set(Some(vp));
    };

    view! {
        <canvas
            class="gantt"
            node_ref=canvas
            style:cursor=move || if hover.get().is_some() { "pointer" } else { "default" }
            on:pointerdown=down
            on:pointermove=moved
            on:pointerup=up
            on:pointercancel=cancelled
            on:pointerleave=move |ev| {
                if ev.pointer_type() == "mouse" {
                    hover.set(None);
                }
            }
            on:wheel=wheel
        ></canvas>
    }
}

/// Whether the item has a dependency line to or from the selected one
fn linked_to(links: Callback<Mark, Vec<Link>>, selected: Mark, node: PlanNode) -> bool {
    links.run(selected).iter().any(|l| {
        matches!(l.role, Role::Before | Role::After)
            && [l.from, l.to].iter().any(|a| match a {
                links::Anchor::Start(n) | links::Anchor::End(n) => {
                    *n == node && selected != Mark::Node(node)
                }
                links::Anchor::PlanStart => false,
            })
    })
}

/// Scrolls the chart while a handle is dragged to its edge
fn edge_scroll(
    viewport: RwSignal<Option<Viewport>>,
    bounds: Signal<Bounds>,
    pointer: RwSignal<(f64, f64)>,
    drag: RwSignal<Option<Drag>>,
) {
    let (Some(Some(_)), Some(b), Some((x, y))) = (
        drag.try_get_untracked(),
        bounds.try_get_untracked(),
        pointer.try_get_untracked(),
    ) else {
        return;
    };
    let push = |at: f64, size: f64, lead: f64| {
        if at < lead + shift::EDGE {
            12.0
        } else if at > size - shift::EDGE {
            -12.0
        } else {
            0.0
        }
    };
    let (dx, dy) = (
        push(x, b.width, 0.0),
        push(y, b.height, layout::HEADER_HEIGHT),
    );
    if dx != 0.0 || dy != 0.0 {
        let mut vp = viewport.try_get_untracked().flatten().unwrap_or_default();
        vp.drag(dx, dy, b);
        viewport.try_set(Some(vp));
    }
    request_animation_frame(move || edge_scroll(viewport, bounds, pointer, drag));
}

/// Keeps the chart moving after a flick, slowing down each frame
fn flick(
    viewport: RwSignal<Option<Viewport>>,
    bounds: Signal<Bounds>,
    gesture: StoredValue<Gesture, LocalStorage>,
    generation: u32,
    (vx, vy): (f64, f64),
) {
    let (vx, vy) = (vx * FRICTION, vy * FRICTION);
    let stale = gesture
        .try_with_value(|g| g.generation != generation)
        .unwrap_or(true);
    if stale || (vx.abs() < 0.5 && vy.abs() < 0.5) {
        return;
    }
    let (Some(current), Some(b)) = (viewport.try_get_untracked(), bounds.try_get_untracked())
    else {
        return;
    };
    let mut vp = current.unwrap_or_default();
    vp.drag(vx * 0.016, vy * 0.016, b);
    viewport.try_set(Some(vp));
    request_animation_frame(move || flick(viewport, bounds, gesture, generation, (vx, vy)));
}

/// The colours an outline can flash in, as red green blue
const RED: &str = "229,57,53";
const LIGHT: &str = "200,200,205";

type Flash = RwSignal<Option<(Mark, f64, &'static str)>>;

/// Lets an outline fade away
fn fade_item(flash: Flash, mark: Mark, rgb: &'static str) {
    fn fade(flash: Flash, mark: Mark, rgb: &'static str, alpha: f64) {
        if alpha <= 0.0 {
            flash.try_set(None);
            return;
        }
        flash.try_set(Some((mark, alpha, rgb)));
        set_timeout(
            move || fade(flash, mark, rgb, alpha - 0.06),
            Duration::from_millis(70),
        );
    }
    fade(flash, mark, rgb, 1.0);
}

/// Flashes an outline on and off, three times, then lets it fade away
fn flash_item(flash: Flash, mark: Mark) {
    fn step(flash: Flash, mark: Mark, count: u32) {
        if count < 5 {
            flash.try_set(count.is_multiple_of(2).then_some((mark, 1.0, RED)));
            set_timeout(
                move || step(flash, mark, count + 1),
                Duration::from_millis(500),
            );
        } else {
            fade_item(flash, mark, RED);
        }
    }
    set_timeout(move || step(flash, mark, 0), Duration::from_millis(300));
}
