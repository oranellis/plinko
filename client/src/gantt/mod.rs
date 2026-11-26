//! The Gantt chart, one canvas as in Plinko 1. Dragging pans it with momentum, the wheel scrolls
//! it, shift and the wheel (or a pinch) zooms, hovering shows an item's dependencies and a panel
//! about it, and tapping opens it.

// modules {{{
pub mod arrange;
pub mod draw;
pub mod info;
pub mod layers;
pub mod layout;
pub mod links;
pub mod pen;
pub mod scene;
pub mod shift;

use std::{collections::HashMap, time::Duration};

use chrono::NaiveDate;
use leptos::{html, prelude::*};
use plinko::planner::items::dependency::PlanNode;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CanvasRenderingContext2d, PointerEvent, WheelEvent};

use crate::gantt::{
    arrange::{Arrangement, Mark},
    draw::{Frame, Hits, paint_header, paint_links, paint_marks, paint_notes},
    layout::{Bounds, DAY_WIDTH, Rect, Viewport},
    links::{Link, Role},
    scene::Scene,
    shift::{Drag, DragView, Editing, Grips, Handle, Held, Overlay, Pin, PinView},
};
// }}}

// constants {{{
/// Movement under this many pixels is a tap, not a drag (more for a finger)
const TAP_MOUSE: f64 = 4.0;
const TAP_TOUCH: f64 = 10.0;
/// What is left of a flick each frame
const FRICTION: f64 = 0.85;
// }}}

/// What is not saved yet of the item being created or edited
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Staged {
    /// What it waits for and what waits for it
    pub before: Vec<Mark>,
    pub after: Vec<Mark>,
    /// The group it is in
    pub group: Option<Mark>,
    /// The item itself when it is on the chart, whose handles then show without shift
    pub item: Option<Mark>,
    /// What has changed of its days, as `+2 (6d)` or `start -1`
    pub days: Option<String>,
    /// The days its left and right edges are moved by, which a ghost of it shows
    pub edges: (i32, i32),
    /// The day it is held to, and whether it can be held to one, which gives it flags to drag
    pub pin: Option<Pin>,
    pub pins: bool,
    /// The dot of the item that clicks on others answer to: the right one, or the left
    pub pick: Option<bool>,
}

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

// now {{{
fn now() -> f64 {
    js_sys::Date::now()
}
// }}}

// Gantt {{{
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
    /// Double clicking an item
    on_edit: Callback<PlanNode>,
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
    /// What an item being edited waits for and what waits for it, outlined in place of the links
    /// of what is selected
    #[prop(optional, into)]
    staged: Option<Signal<Option<Staged>>>,
    /// An item to outline as if the pointer were on it, for a drag that began off the chart
    #[prop(optional, into)]
    spot: Option<Signal<Option<Mark>>>,
    /// Set to a way of asking what item is at a point of the window, while the chart is there
    #[prop(optional)]
    probe: Option<crate::card::Probe>,
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
    let grabbed_day = StoredValue::new_local(0.0);

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
        let (start, today) = (scene.with_untracked(|s| s.origin), today.get_untracked());
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
            Some(((placed.span.0 - s.origin).num_days() as f64, placed.lane))
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
                    Some(((placed.span.0 - s.origin).num_days() as f64, placed.lane))
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

    // what is under the pointer
    let hit = move |x: f64, y: f64| hits.with_value(|h| h.hit(x, y));
    if let Some(probe) = probe {
        probe.set(Some(Callback::new(move |(x, y): (f64, f64)| {
            let canvas = canvas.try_get_untracked()??;
            let rect = AsRef::<web_sys::Element>::as_ref(&canvas).get_bounding_client_rect();
            let inside = (rect.left()..=rect.right()).contains(&x)
                && (rect.top()..=rect.bottom()).contains(&y);
            let found = inside.then(|| hit(x - rect.left(), y - rect.top()));
            found.map(|item| item.map(|(mark, _)| mark))
        })));
        on_cleanup(move || {
            let _ = probe.try_set(None);
        });
    }
    // where an item is drawn, a group by its name strip
    let rect_of = move |mark: Mark| -> Option<Rect> {
        hits.with_value(|h| {
            h.items()
                .find(|(m, _)| *m == mark)
                .map(|(_, r)| r)
                .or_else(|| h.headers().find(|(m, ..)| *m == mark).map(|(_, r, _)| r))
        })
    };
    // how far the edges of the item being edited are moved, in days
    let edges_of = move |node: PlanNode| {
        let staged = staged.and_then(|s| {
            s.with_untracked(|s| s.as_ref().filter(|s| s.item == Some(node)).map(|s| s.edges))
        });
        staged.unwrap_or_default()
    };
    // where an item is once what is staged of its edges is done, which is where its handles are
    let staged_rect = move |node: PlanNode| -> Option<Rect> {
        let (r, (left, right)) = (rect_of(node)?, edges_of(node));
        let day = viewport.get_untracked().unwrap_or_default().day_width;
        let (left, right) = (f64::from(left) * day, f64::from(right) * day);
        Some(Rect {
            x: r.x + left,
            w: (r.w - left + right).max(layout::MIN_BAR_WIDTH),
            ..r
        })
    };
    // where a pin of an item is drawn
    let pin_view = move |node: PlanNode, pin: Pin, faint: bool| -> Option<PinView> {
        let row = staged_rect(node)?;
        let vp = viewport.get_untracked().unwrap_or_default();
        let day = scene.with_untracked(|s| (pin.day - s.origin).num_days()) as f64;
        Some(PinView {
            x: vp.x(day + if pin.end { 1.0 } else { 0.0 }),
            row,
            cell: Rect {
                x: vp.x(day),
                y: layout::MONTH_HEIGHT,
                w: vp.day_width,
                h: layout::HEADER_HEIGHT - layout::MONTH_HEIGHT,
            },
            held: pin.held,
            faint,
        })
    };
    // what is staged of the item being edited, if it is the one
    let staged_of = move |node: PlanNode| {
        staged.and_then(|s| s.with_untracked(|s| s.clone().filter(|s| s.item == Some(node))))
    };
    // the handles of the item the pointer is near, with flags for the day the one being edited
    // is held to: on its pin, or over the edge they hold
    let handles_of = move |node: PlanNode| {
        let kind = scene.with_untracked(|s| s.placed(node).map(|p| p.kind));
        let (Some(kind), Some(rect)) = (kind, staged_rect(node)) else {
            return Grips::default();
        };
        let mut grips = Grips::of(kind, rect, editing.dated.run(node));
        if let Some(staged) = staged_of(node).filter(|s| s.pins) {
            // a milestone is a day, held by its end
            let ends: &[bool] = match kind {
                arrange::Kind::Milestone => &[true],
                _ => &[false, true],
            };
            for end in ends {
                let home = rect.x + if *end { rect.w } else { 0.0 };
                let on_pin = staged
                    .pin
                    .filter(|p| p.end == *end)
                    .and_then(|p| pin_view(node, p, false));
                let x = on_pin.map_or(home, |p| p.x);
                grips
                    .list
                    .push((Handle::Flag { end: *end }, (x, rect.y - 9.0)));
            }
        }
        grips
    };
    // the pin a flag being dragged would leave on the day under the pointer, of the kind the
    // item has, or for a first one not before the day for a start and not after it for an end
    let flagged = move |node: PlanNode, end: bool, x: f64| -> Pin {
        let offset = viewport
            .get_untracked()
            .unwrap_or_default()
            .day_at(x)
            .floor() as i64;
        let day = scene.with_untracked(|s| s.origin + chrono::Duration::days(offset));
        let was = staged_of(node).and_then(|s| s.pin);
        let held = was.map_or(if end { Held::Latest } else { Held::Earliest }, |p| p.held);
        Pin { day, held, end }
    };

    // what the lines and the panel say about what is under the pointer, which only changes with
    // that, so drawing again for a pan or a nudge of the pointer does not work it out again
    let linked = Memo::new(move |_| {
        let (hovered, picked) = (hover.get(), selected.get());
        // with something selected, the chain to what is under the pointer shows instead
        let chain = match (picked, hovered) {
            (Some(a), Some(b)) if a != b => editing.chain.run((a, b)),
            _ => Vec::new(),
        };
        if chain.is_empty() {
            hovered.or(picked).map(|m| links.run(m)).unwrap_or_default()
        } else {
            chain
        }
    });
    let said = Memo::new(move |_| hover.get().map(|m| info.run(m)).unwrap_or_default());

    let layers = StoredValue::new_local(None::<layers::Layers>);
    let scene_gen = StoredValue::new_local(0u64);
    // the layers are only drawn again for a scene that has something different on it
    let drawn = StoredValue::new_local(None::<(NaiveDate, Arrangement)>);
    Effect::new(move |_| {
        scene.with(|s| {
            let same = drawn.with_value(|d| {
                d.as_ref().is_some_and(|(origin, arrangement)| {
                    *origin == s.origin && *arrangement == s.arrangement
                })
            });
            if !same {
                drawn.set_value(Some((s.origin, s.arrangement.clone())));
                scene_gen.update_value(|g| *g += 1);
            }
        });
    });
    let surface = StoredValue::new_local(None::<((u32, u32), std::rc::Rc<pen::Pen>)>);
    // draws what is on the chart, once a frame however many things changed since the last
    let draw = move || {
        let Some((width, height)) = size.try_get_untracked() else {
            return;
        };
        let picked = selected.get();
        let staged = staged.and_then(|s| s.get());
        let (vp, hovered, flashed, today) = (
            viewport.get().unwrap_or_default(),
            hover.get().or_else(|| spot.and_then(|s| s.get())),
            flash.get(),
            today.get(),
        );
        let at = pointer.get();
        let pointer_at = at;
        let (editing_shift, near, dragged) = (shift.get(), nearby.get(), drag.get());
        // the item being edited keeps its handles, shift shows those of the item the pointer is near
        let pinned = staged.as_ref().and_then(|s| s.item);
        let gripped = near.filter(|_| editing_shift).or(pinned);
        let Some(canvas) = canvas.get() else {
            return;
        };
        let ratio = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
        let pixels = ((width * ratio) as u32, (height * ratio) as u32);
        // resizing a canvas throws its surface away and resets what was set on it, so that only
        // happens when the size changes and the drawing carries on with what it remembers
        let kept = surface.with_value(|s| {
            s.as_ref()
                .filter(|(size, _)| *size == pixels)
                .map(|(_, pen)| pen.clone())
        });
        let ctx = match kept {
            Some(pen) => pen,
            None => {
                canvas.set_width(pixels.0);
                canvas.set_height(pixels.1);
                let Some(Ok(ctx)) = canvas
                    .get_context("2d")
                    .ok()
                    .flatten()
                    .map(|c| c.dyn_into::<CanvasRenderingContext2d>())
                else {
                    return;
                };
                let _ = ctx.set_transform(ratio, 0.0, 0.0, ratio, 0.0, 0.0);
                let pen = std::rc::Rc::new(pen::Pen::new(ctx));
                surface.set_value(Some((pixels, pen.clone())));
                pen
            }
        };
        let ctx: &pen::Pen = &ctx;
        let (linked, info) = (linked.get(), said.get());
        let generation = scene_gen.get_value();
        // the layers are drawn again only when the chart itself changes or it is scrolled far
        let fresh = layers.with_value(|l| {
            l.as_ref()
                .is_some_and(|l| l.serve(vp, (width, height), ratio, generation, today))
        });
        if !fresh {
            let old = layers.try_update_value(Option::take).flatten();
            let made = scene.with(|scene| {
                layers::render(old, (width, height), ratio, vp, scene, generation, today)
            });
            if let Some((made, found)) = made {
                hits.set_value(found.hits);
                layers.set_value(Some(made));
            }
        }
        layers.with_value(|layers| {
            let Some(layers) = layers else {
                return;
            };
            let at = layers.at(vp);
            hits.update_value(|h| h.offset = at);
            scene.with(|scene| {
                let over_violation = hovered.filter(|m| {
                    hits.with_value(|h| {
                        h.violations()
                            .any(|(v, r)| v == *m && r.contains(pointer_at.0, pointer_at.1))
                    })
                });
                let tip = over_violation.map(|m| violation.run(m));
                let frame = Frame {
                    width,
                    height,
                    top: layout::HEADER_HEIGHT,
                    viewport: vp,
                    scene,
                    today,
                    hover: hovered,
                    selected: picked,
                    staged: staged.as_ref(),
                    nameless: gripped,
                    links: &linked,
                    flash: flashed,
                    info: if over_violation.is_some() { &[] } else { &info },
                    tip: tip.as_deref().map(|lines| (lines, pointer_at)),
                };
                // behind the items, what the pointer is on shows its lines, then the items over
                // them, then the outlines and names the pointer changes, then the header
                layers.blit(ctx, at, true);
                paint_links(ctx, &frame);
                layers.blit(ctx, at, false);
                paint_marks(ctx, &frame, &layers.labels, at);
                paint_header(ctx, &frame);
                paint_notes(ctx, &frame);
            });
        });
        // the day what is looked at is held to, faintly, and that of the item being edited
        if let Some(looked) = hovered.or(picked).filter(|n| Some(*n) != pinned) {
            let pin = editing
                .pin
                .run(looked)
                .and_then(|p| pin_view(looked, p, true));
            if let Some(pin) = pin {
                shift::paint_pin(ctx, &pin);
            }
        }
        let flag_dragged = matches!(dragged, Some(Drag::Flag { .. }));
        let held = staged
            .as_ref()
            .and_then(|s| s.pin)
            .filter(|_| !flag_dragged);
        if let Some(pin) = pinned.zip(held).and_then(|(n, p)| pin_view(n, p, false)) {
            shift::paint_pin(ctx, &pin);
        }
        if editing_shift || gripped.is_some() {
            let handles = gripped.map(handles_of).unwrap_or_default();
            let note = staged
                .as_ref()
                .and_then(|s| Some((staged_rect(s.item?)?, s.days.clone()?)));
            let ghost = pinned
                .filter(|node| edges_of(*node) != (0, 0))
                .and_then(staged_rect);
            let target = |own: PlanNode| {
                hit(at.0, at.1)
                    .map(|(m, _)| m)
                    .filter(|m| *m != own)
                    .and_then(rect_of)
            };
            let view = dragged.map(|d| match d {
                Drag::Link { node, right } => DragView::Link {
                    from: handles
                        .list
                        .iter()
                        .find(|(h, _)| *h == Handle::Link { right })
                        .map_or(at, |(_, p)| *p),
                    over: target(node),
                    right,
                },
                Drag::Move(node) => {
                    let (from_x, from_y) = grabbed_at.get_value();
                    let name = scene.with_untracked(|s| {
                        s.arrangement
                            .placed
                            .iter()
                            .find(|p| p.mark == node)
                            .map(|p| p.name.clone())
                            .unwrap_or_default()
                    });
                    let sideways = editing.dated.run(node) && target(node).is_none();
                    let by = if sideways {
                        let days = day_shift(viewport, at.0, grabbed_day.get_value());
                        (
                            f64::from(days)
                                * viewport.get_untracked().unwrap_or_default().day_width,
                            0.0,
                        )
                    } else {
                        (at.0 - from_x, at.1 - from_y)
                    };
                    DragView::Move {
                        over: target(node),
                        origin: rect_of(node).unwrap_or(Rect {
                            x: at.0,
                            y: at.1,
                            w: 0.0,
                            h: 0.0,
                        }),
                        by,
                        name,
                    }
                }
                Drag::Resize {
                    node,
                    last_day,
                    duration,
                } => {
                    let vp = viewport.get_untracked().unwrap_or_default();
                    let (delta, new) = shift::resized(vp.day_at(at.0), last_day, duration);
                    let origin = staged_rect(node).unwrap_or(Rect {
                        x: at.0,
                        y: at.1,
                        w: 0.0,
                        h: 0.0,
                    });
                    DragView::Resize {
                        label: shift::resize_label(delta, new),
                        origin,
                        grown: Rect {
                            w: (origin.w + f64::from(delta) * vp.day_width)
                                .max(layout::MIN_BAR_WIDTH),
                            ..origin
                        },
                    }
                }
                Drag::Flag { node, end } => match pin_view(node, flagged(node, end, at.0), false) {
                    Some(pin) => DragView::Flag(pin),
                    None => DragView::Link {
                        from: at,
                        over: None,
                        right: end,
                    },
                },
                Drag::Start { node, first_day } => {
                    let vp = viewport.get_untracked().unwrap_or_default();
                    let by = f64::from(day_shift(viewport, at.0, first_day as f64)) * vp.day_width;
                    let origin = staged_rect(node).unwrap_or(Rect {
                        x: at.0,
                        y: at.1,
                        w: 0.0,
                        h: 0.0,
                    });
                    DragView::Resize {
                        label: format!("{:+}", day_shift(viewport, at.0, first_day as f64)),
                        origin,
                        grown: Rect {
                            x: origin.x + by.min(origin.w - layout::MIN_BAR_WIDTH),
                            w: (origin.w - by).max(layout::MIN_BAR_WIDTH),
                            ..origin
                        },
                    }
                }
            });
            shift::paint(
                ctx,
                &Overlay {
                    handles,
                    pointer: at,
                    drag: view,
                    note,
                    ghost,
                    picked: staged.as_ref().and_then(|s| s.pick),
                },
            );
        }
    };
    let pinned = move || staged.and_then(|s| s.with_untracked(|s| s.as_ref()?.item));
    let due = StoredValue::new_local(false);
    Effect::new(move |_| {
        // the things a drawing depends on, and a drawing is asked for when any of them changes
        let hovered = hover.get();
        let (dragging, shifted) = (drag.get().is_some(), shift.get());
        let _ = (
            size.track(),
            selected.track(),
            viewport.track(),
            flash.track(),
            today.track(),
            nearby.track(),
            scene.track(),
            canvas.track(),
            linked.track(),
            said.track(),
            staged.map(|s| s.track()),
            spot.map(|s| s.track()),
        );
        // the pointer matters while something follows it: a drag, the handles, a note on a warning
        let warns = hovered
            .is_some_and(|m| scene.with_untracked(|s| s.placed(m).is_some_and(|p| p.violated)));
        if dragging || shifted || warns || pinned().is_some() {
            pointer.track();
        }
        if !due.get_value() {
            due.set_value(true);
            request_animation_frame(move || {
                if due.try_set_value(false).is_none() {
                    untrack(draw);
                }
            });
        }
    });

    let down = move |ev: PointerEvent| {
        // a handle starts editing instead of panning: with shift, or of the item being edited
        if ev.pointer_type() == "mouse" {
            let (x, y) = (f64::from(ev.offset_x()), f64::from(ev.offset_y()));
            let gripped = nearby
                .get_untracked()
                .filter(|_| ev.shift_key())
                .or_else(pinned);
            if let Some((node, handle)) =
                gripped.and_then(|node| handles_of(node).at(x, y).map(|h| (node, h)))
            {
                if let Some(canvas) = canvas.get_untracked() {
                    let element: &web_sys::Element = canvas.as_ref();
                    let _ = element.set_pointer_capture(ev.pointer_id());
                }
                grabbed_at.set_value((x, y));
                grabbed_day.set_value(
                    viewport
                        .get_untracked()
                        .unwrap_or_default()
                        .day_at(x)
                        .floor(),
                );
                drag.set(Some(match handle {
                    Handle::Link { right } => Drag::Link { node, right },
                    Handle::Move => Drag::Move(node),
                    Handle::Flag { end } => Drag::Flag { node, end },
                    Handle::Start => {
                        let first = scene.with_untracked(|s| {
                            s.placed(node).map(|p| (p.span.0 - s.origin).num_days())
                        });
                        Drag::Start {
                            node,
                            first_day: first.unwrap_or(0) + i64::from(edges_of(node).0),
                        }
                    }
                    Handle::Resize => {
                        let end = scene.with_untracked(|s| {
                            s.arrangement
                                .placed
                                .iter()
                                .find(|p| p.mark == node)
                                .map(|p| (s.origin, p.span.1))
                        });
                        let last_day = end.map_or(0, |(start, end)| (end - start).num_days() - 1)
                            + i64::from(edges_of(node).1);
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
                .is_some_and(|n| handles_of(n).at(x, y).is_some());
            if !on_handle {
                let found = hits.with_value(|h| {
                    h.items()
                        .chain(h.headers().map(|(m, r, _)| (m, r)))
                        .find(|(_, r)| shift::near(*r, x, y))
                        .map(|(m, _)| m)
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
        if let Some((node, fold)) = hit(x, y) {
            let on_violation = hits.with_value(|h| h.violations().any(|(_, r)| r.contains(x, y)));
            let chosen = selected.get_untracked();
            match (fold, on_violation) {
                (_, true) => {}
                (true, false) => on_toggle.run(node),
                (false, false) if shifted && chosen.is_some_and(|s| linked_to(links, s, node)) => {
                    if let Some(own) = chosen {
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
                match done {
                    Drag::Link { node, right } => editing.pick.run((node, right)),
                    _ => tap(x, y, ev.shift_key()),
                }
                return;
            }
            let over = hit(x, y).map(|(m, _)| m);
            match done {
                Drag::Link { node, right } => {
                    if let Some(other) = over.filter(|o| *o != node) {
                        editing.link.run((node, other, right));
                    }
                }
                Drag::Move(node) => {
                    // something started goes sideways in its dates, unless dropped on another item
                    let by = day_shift(viewport, x, grabbed_day.get_value());
                    if editing.dated.run(node) && over.is_none() {
                        if by != 0 {
                            editing.shift_dates.run((node, by));
                        }
                    } else if over != Some(node) {
                        editing.moved.run((node, over));
                    }
                }
                Drag::Flag { node, end } => {
                    editing
                        .constrain
                        .run((node, end, flagged(node, end, x).day));
                }
                Drag::Start { node, first_day } => {
                    let by = day_shift(viewport, x, first_day as f64);
                    if by != 0 {
                        editing.restart.run((node, by));
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

    let slide = StoredValue::new_local(crate::glide::Glide::new(move |dx| {
        let (Some(current), Some(b)) = (viewport.try_get_untracked(), bounds.try_get_untracked())
        else {
            return false;
        };
        let mut vp = current.unwrap_or_default();
        vp.scroll(dx, 0.0, b);
        viewport.try_set(Some(vp));
        true
    }));
    let wheel = move |ev: WheelEvent| {
        ev.prevent_default();
        let b = bounds.get_untracked();
        let x = f64::from(ev.offset_x());
        let zoom = if ev.shift_key() {
            Some(if ev.delta_y() > 0.0 { 0.9 } else { 1.1 })
        } else if ev.ctrl_key() || ev.meta_key() {
            Some((-ev.delta_y() / 300.0).exp())
        } else {
            None
        };
        if let Some(factor) = zoom {
            let mut vp = viewport.get_untracked().unwrap_or_default();
            vp.zoom(x, factor, b);
            viewport.set(Some(vp));
            return;
        }
        // sideways follows the gesture and carries on when it lets go, which moves the chart
        // itself, so up and down is applied to the chart as it is after that
        slide.with_value(|s| s.push(ev.delta_x()));
        let mut vp = viewport.get_untracked().unwrap_or_default();
        vp.scroll(0.0, ev.delta_y(), b);
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
            on:dblclick=move |ev| {
                if let Some((node, false)) = hit(f64::from(ev.offset_x()), f64::from(ev.offset_y())) {
                    on_edit.run(node);
                }
            }
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
// }}}

// helpers {{{
/// The whole days the pointer at `x` is from the day that was grabbed
fn day_shift(viewport: RwSignal<Option<Viewport>>, x: f64, grabbed: f64) -> i32 {
    (viewport
        .get_untracked()
        .unwrap_or_default()
        .day_at(x)
        .floor()
        - grabbed) as i32
}

/// Whether the item has a dependency line to or from the selected one
fn linked_to(links: Callback<Mark, Vec<Link>>, selected: Mark, node: PlanNode) -> bool {
    links.run(selected).iter().any(|l| {
        matches!(l.role, Role::Before | Role::After)
            && [l.from, l.to].iter().any(|a| match a {
                links::Anchor::Start(n) | links::Anchor::End(n) => *n == node && selected != node,
            })
    })
}
// }}}

// edge_scroll {{{
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
// }}}

// flick {{{
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
// }}}

// constants {{{
/// The colours an outline can flash in, as red green blue
const RED: &str = "229,57,53";
const LIGHT: &str = "200,200,205";
// }}}

// Gesture supporting types {{{
type Flash = RwSignal<Option<(Mark, f64, &'static str)>>;
// }}}

// helpers {{{
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
// }}}
