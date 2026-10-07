//! The Gantt chart: a canvas that pans by dragging, zooms with the wheel or a pinch, and selects
//! a row by tapping it.

pub mod draw;
pub mod layout;

use std::collections::HashMap;

use chrono::NaiveDate;
use leptos::{html, prelude::*};
use plinko::types::dependency::PlanNode;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CanvasRenderingContext2d, PointerEvent, WheelEvent};

use crate::{
    gantt::{
        draw::{LABEL_WIDTH, LABEL_WIDTH_NARROW, Scene, paint},
        layout::{View, ROW_HEIGHT},
    },
    model::Row,
};

/// A finger or the mouse that is down
#[derive(Clone, Copy)]
struct Touch {
    x: f64,
    y: f64,
}

#[derive(Default)]
struct Gesture {
    touches: HashMap<i32, Touch>,
    /// How far everything has moved since the first touch, to tell a tap from a drag
    travelled: f64,
}

fn label_width(width: f64) -> f64 {
    if width < 600.0 { LABEL_WIDTH_NARROW } else { LABEL_WIDTH }
}

#[component]
pub fn Gantt(
    rows: Signal<Vec<Row>>,
    today: Signal<NaiveDate>,
    /// The days to show at first
    range: Signal<Option<(NaiveDate, NaiveDate)>>,
    selected: Signal<Option<PlanNode>>,
    on_select: Callback<PlanNode>,
    /// A group's arrow was tapped
    on_toggle: Callback<PlanNode>,
) -> impl IntoView {
    let canvas = NodeRef::<html::Canvas>::new();
    let size = RwSignal::new((800.0, 500.0));
    let view = RwSignal::new(View::fit(
        NaiveDate::from_ymd_opt(2024, 1, 1).expect("a date"),
        NaiveDate::from_ymd_opt(2024, 2, 1).expect("a date"),
        800.0,
    ));
    let fitted = StoredValue::new(false);
    let gesture = StoredValue::new_local(Gesture::default());

    let measure = move || {
        if let Some(canvas) = canvas.get_untracked() {
            let element: &web_sys::Element = canvas.as_ref();
            size.set((f64::from(element.client_width()), f64::from(element.client_height())));
        }
    };
    let on_resize = Closure::<dyn FnMut()>::new(measure);
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref());
    }
    on_resize.forget();
    Effect::new(move |_| {
        if canvas.get().is_some() {
            measure();
        }
    });

    // show the whole plan the first time there is one
    Effect::new(move |_| {
        let (width, _) = size.get();
        if let (false, Some((first, last))) = (fitted.get_value(), range.get()) {
            fitted.set_value(true);
            view.set(View::fit(first, last, width - label_width(width)));
        }
    });

    Effect::new(move |_| {
        let (width, height) = size.get();
        let (rows, view, selected, today) = (rows.get(), view.get(), selected.get(), today.get());
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
        let scene = Scene {
            width,
            height,
            label_width: label_width(width),
            view,
            rows: &rows,
            selected,
            today,
        };
        paint(&ctx, &scene);
    });

    let down = move |ev: PointerEvent| {
        if let Some(canvas) = canvas.get_untracked() {
            let element: &web_sys::Element = canvas.as_ref();
            let _ = element.set_pointer_capture(ev.pointer_id());
        }
        gesture.update_value(|g| {
            if g.touches.is_empty() {
                g.travelled = 0.0;
            }
            g.touches.insert(ev.pointer_id(), Touch { x: f64::from(ev.client_x()), y: f64::from(ev.client_y()) });
        });
    };

    let moved = move |ev: PointerEvent| {
        let (width, height) = size.get_untracked();
        let count = rows.with_untracked(Vec::len);
        let label = label_width(width);
        let now = Touch { x: f64::from(ev.client_x()), y: f64::from(ev.client_y()) };
        gesture.update_value(|g| {
            let Some(before) = g.touches.get(&ev.pointer_id()).copied() else {
                return;
            };
            let others: Vec<Touch> = g
                .touches
                .iter()
                .filter(|(id, _)| **id != ev.pointer_id())
                .map(|(_, t)| *t)
                .collect();
            g.travelled += (now.x - before.x).abs() + (now.y - before.y).abs();
            view.update(|v| match others.as_slice() {
                [] => v.pan(now.x - before.x, now.y - before.y, count, height),
                [other, ..] => {
                    // two fingers: the distance between them is the zoom
                    let (old, new) = (
                        (before.x - other.x).hypot(before.y - other.y),
                        (now.x - other.x).hypot(now.y - other.y),
                    );
                    if old > 1.0 {
                        let middle = (now.x + other.x) / 2.0 - label;
                        v.zoom(middle.max(0.0), new / old);
                    }
                }
            });
            g.touches.insert(ev.pointer_id(), now);
        });
    };

    let up = move |ev: PointerEvent| {
        let (width, _) = size.get_untracked();
        let tapped = gesture
            .try_update_value(|g| {
                let tap = g.touches.len() == 1 && g.travelled < 6.0;
                g.touches.remove(&ev.pointer_id());
                tap
            })
            .unwrap_or(false);
        if !tapped {
            return;
        }
        let (x, y) = (f64::from(ev.offset_x()), f64::from(ev.offset_y()));
        let list = rows.get_untracked();
        let Some(index) = view.get_untracked().row_at(y, list.len()) else {
            return;
        };
        let row = &list[index];
        let arrow = 8.0 + f64::from(row.depth) * 14.0;
        if x < label_width(width) && row.has_members && x >= arrow - 4.0 && x <= arrow + 16.0 {
            on_toggle.run(row.node);
        } else {
            on_select.run(row.node);
        }
    };

    let cancelled = move |ev: PointerEvent| {
        gesture.update_value(|g| {
            g.touches.remove(&ev.pointer_id());
        });
    };

    let wheel = move |ev: WheelEvent| {
        ev.prevent_default();
        let (width, height) = size.get_untracked();
        let count = rows.with_untracked(Vec::len);
        let x = f64::from(ev.offset_x()) - label_width(width);
        view.update(|v| {
            if ev.ctrl_key() || ev.meta_key() {
                v.zoom(x.max(0.0), (-ev.delta_y() / 300.0).exp());
            } else if ev.shift_key() {
                v.pan(-ev.delta_y(), 0.0, count, height);
            } else {
                v.pan(-ev.delta_x(), -ev.delta_y(), count, height);
            }
        });
    };

    let _ = ROW_HEIGHT;
    view! {
        <canvas
            class="gantt"
            node_ref=canvas
            on:pointerdown=down
            on:pointermove=moved
            on:pointerup=up
            on:pointercancel=cancelled
            on:wheel=wheel
        ></canvas>
    }
}
