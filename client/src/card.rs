//! The card at the bottom of a plan for the item being created or edited: its shape, with handles
//! for what it waits for, what waits for it, the group it is in and how many days it lasts. What
//! the handles change is staged in the page's `Draft` and saved with the form beside it.

// modules {{{
use leptos::{ev::PointerEvent, html, prelude::*};
use plinko::{
    common::short_id::ShortId,
    planner::items::{dependency::PlanNode, parent::Parent},
    planner::plan::Plan,
};
use wasm_bindgen::JsCast;

use crate::{
    draft::{Draft, Pick},
    model::{self, node_name},
    ui::use_sync,
};
// }}}

// constants {{{
/// How far a handle moves before letting go of it is a drop and not a click
const CLICK: f64 = 5.0;

/// How far the days handle is dragged for each day
const DAY_STEP: f64 = 14.0;
/// The most boxes shown for the days while they are dragged
const MOST_BOXES: u32 = 30;

/// The most days an item can be given
const MOST_DAYS: i64 = 3650;
// }}}

/// How to ask the chart what is at a point of the window: `None` off the chart, `Some(None)` on
/// a part of it with no item
pub type Probe = RwSignal<Option<Callback<(f64, f64), Option<Option<PlanNode>>>>>;

// Probe supporting types {{{
/// A handle of the card
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// A circle, for what the item waits for or what waits for it
    Link(Pick),
    /// The grip in the middle, for the group it is in
    Move,
    /// The bar on its right edge, for its days
    Days,
}

/// A handle that is held down: where it was taken, where the pointer is now, the days the item
/// had then, and whether a mouse holds it (a finger only clicks, dragging is for panning)
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    grab: Grab,
    from: (f64, f64),
    at: (f64, f64),
    days: u32,
    mouse: bool,
}
// }}}

// impl Held {{{
impl Held {
    fn dragged(&self) -> bool {
        self.mouse && (self.at.0 - self.from.0).hypot(self.at.1 - self.from.1) >= CLICK
    }
}
// }}}

// helpers {{{
/// The fewest days a kind of item can last
fn least_days(node: PlanNode) -> i64 {
    match node {
        PlanNode::Task(_) => 1,
        _ => 0,
    }
}

fn kind_class(node: PlanNode) -> &'static str {
    match node {
        PlanNode::Task(_) => "task",
        PlanNode::Milestone(_) => "milestone",
        PlanNode::TaskGroup(_) => "group",
        PlanNode::PlanBuffer(_) => "buffer",
        PlanNode::PlanDelay(_) => "delay",
    }
}

fn new_name(node: PlanNode) -> &'static str {
    match node {
        PlanNode::Task(_) => "New task",
        PlanNode::Milestone(_) => "New milestone",
        PlanNode::TaskGroup(_) => "New group",
        PlanNode::PlanBuffer(_) => "Buffer",
        PlanNode::PlanDelay(_) => "Delay",
    }
}
// }}}

// Card {{{
/// The card for the page's draft, shown while there is one. `probe` is how drops and `spot` how
/// the item under a drag reach the chart, with no chart the handles that need one are left out.
#[component]
pub fn Card(
    plan: ShortId,
    draft: RwSignal<Option<Draft>>,
    #[prop(optional)] probe: Option<Probe>,
    #[prop(optional)] spot: Option<RwSignal<Option<PlanNode>>>,
    /// The draft was changed from the card
    #[prop(optional)]
    on_change: Option<Callback<()>>,
) -> impl IntoView {
    let sync = use_sync();
    let held = RwSignal::new(None::<Held>);
    let typing = RwSignal::new(false);
    let days_box = NodeRef::<html::Input>::new();
    let with_plan = move |f: &mut dyn FnMut(&Plan)| {
        sync.tenant.with_untracked(|t| {
            if let Some(p) = t.as_ref().and_then(|t| t.plans().get(&plan)) {
                f(p);
            }
        });
    };
    let changed = move || {
        if let Some(on_change) = on_change {
            on_change.run(());
        }
    };
    // changes the draft, and says why if the plan would not take the change
    let change = move |f: &dyn Fn(&mut Draft, &Plan) -> Result<(), String>| {
        let mut said = None;
        with_plan(&mut |p| {
            draft.update(|d| {
                if let Some(d) = d {
                    said = f(d, p).err();
                }
            });
        });
        match said {
            Some(why) => sync.notice.set(Some(why)),
            None => changed(),
        }
    };
    let at_point = move |at: (f64, f64)| probe.and_then(|p| p.get_untracked()?.run(at));
    let node = Memo::new(move |_| draft.with(|d| d.as_ref().map(|d| d.node)));
    let lit = move |pick: Pick| draft.with(|d| d.as_ref().is_some_and(|d| d.pick == Some(pick)));
    let days = move || draft.with(|d| d.as_ref().and_then(|d| d.duration));

    let down = move |grab: Grab, ev: PointerEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        if let Some(target) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        let at = (f64::from(ev.client_x()), f64::from(ev.client_y()));
        held.set(Some(Held {
            grab,
            from: at,
            at,
            days: days().unwrap_or(0),
            mouse: ev.pointer_type() == "mouse",
        }));
    };
    let moved = move |ev: PointerEvent| {
        let Some(mut now) = held.get_untracked().filter(|h| h.mouse) else {
            return;
        };
        now.at = (f64::from(ev.client_x()), f64::from(ev.client_y()));
        held.set(Some(now));
        if !now.dragged() {
            return;
        }
        match now.grab {
            Grab::Days => {
                let steps = ((now.at.0 - now.from.0) / DAY_STEP).round() as i64;
                draft.update(|d| {
                    if let Some(d) = d {
                        let wanted = i64::from(now.days) + steps;
                        let days = wanted.clamp(least_days(d.node), MOST_DAYS) as u32;
                        d.duration = d.duration.map(|_| days);
                    }
                });
                changed();
            }
            Grab::Link(_) | Grab::Move => {
                if let Some(spot) = spot {
                    spot.set(at_point(now.at).flatten());
                }
            }
        }
    };
    let up = move |_: PointerEvent| {
        let Some(done) = held.get_untracked() else {
            return;
        };
        held.set(None);
        if let Some(spot) = spot {
            spot.set(None);
        }
        let over = at_point(done.at);
        match (done.grab, done.dragged()) {
            // a click lights the handle, or puts it out, and the chart then answers to it
            (Grab::Link(pick), false) => draft.update(|d| {
                if let Some(d) = d {
                    d.pick = Some(pick).filter(|p| d.pick != Some(*p));
                }
            }),
            (Grab::Move, false) => draft.update(|d| {
                if let Some(d) = d {
                    d.pick = Some(Pick::Group).filter(|p| d.pick != Some(*p));
                }
            }),
            (Grab::Days, false) => typing.set(true),
            (Grab::Link(pick), true) => {
                if let Some(Some(other)) = over {
                    change(&|d, p| d.link(p, pick, other));
                }
            }
            // let go off the chart it stays where it is
            (Grab::Move, true) => {
                if let Some(over) = over {
                    change(&|d, p| d.place(p, over));
                }
            }
            (Grab::Days, true) => {}
        }
    };
    let cancelled = move |_: PointerEvent| {
        held.set(None);
        if let Some(spot) = spot {
            spot.set(None);
        }
    };
    Effect::new(move |_| {
        if typing.get()
            && let Some(input) = days_box.get()
        {
            let _ = input.focus();
            input.select();
        }
    });
    let typed = move || {
        let wanted = days_box
            .get_untracked()
            .and_then(|i| i.value().parse::<i64>().ok());
        typing.set(false);
        draft.update(|d| {
            if let (Some(d), Some(wanted)) = (d, wanted) {
                let days = wanted.clamp(least_days(d.node), MOST_DAYS) as u32;
                d.duration = d.duration.map(|_| days);
            }
        });
        changed();
    };

    let name = move || {
        let Some(d) = draft.get() else {
            return String::new();
        };
        match d.new {
            true => new_name(d.node).to_string(),
            false => sync.tenant.with(|t| {
                let plan = t.as_ref().and_then(|t| t.plans().get(&plan));
                plan.map(|p| node_name(p, d.node)).unwrap_or_default()
            }),
        }
    };
    // what a lit handle is waiting for
    let said = move || {
        draft.with(|d| d.as_ref()?.pick).map(|pick| match pick {
            Pick::Before => "Click the items it waits for",
            Pick::After => "Click the items that wait for it",
            Pick::Group => "Click a group to put it in, or an empty part of the chart",
        })
    };
    // the line from a handle being dragged to the pointer
    let line = move || {
        let h = held.get().filter(|h| h.dragged() && h.grab != Grab::Days)?;
        let tone = match h.grab {
            Grab::Link(Pick::After) => "after",
            Grab::Link(_) => "before",
            _ => "group",
        };
        Some(view! {
            <svg class="draftcard-line">
                <line class=tone x1=h.from.0 y1=h.from.1 x2=h.at.0 y2=h.at.1 />
            </svg>
        })
    };
    let charted = probe.is_some();
    let linked = move |pick: Pick| {
        draft.with(|d| {
            d.as_ref().map_or(0, |d| match pick {
                Pick::After => d.dependents.len(),
                _ => d.dependencies.len(),
            })
        })
    };
    let knob = move |pick: Pick, label: &'static str| {
        let side = if pick == Pick::After {
            "after"
        } else {
            "before"
        };
        charted.then(|| {
            view! {
                <button
                    type="button"
                    class=format!("draftcard-knob {side}")
                    class:lit=move || lit(pick)
                    title=label
                    aria-label=move || format!("{label} ({} linked)", linked(pick))
                    aria-pressed=move || lit(pick).to_string()
                    on:pointerdown=move |ev| down(Grab::Link(pick), ev)
                    on:pointermove=moved
                    on:pointerup=up
                    on:pointercancel=cancelled
                ></button>
            }
        })
    };

    view! {
        <Show when=move || node.get().is_some()>
            <div class="draftcard" on:click=|ev| ev.stop_propagation()>
                <div class="draftcard-stage">
                    {knob(Pick::Before, "What it waits for: drag onto an item, or click and pick items")}
                    <div class=move || format!("draftcard-shape {}", node.get().map_or("", kind_class))>
                        <span class="draftcard-name">{name}</span>
                        {charted.then(|| view! {
                            <button
                                type="button"
                                class="draftcard-grip"
                                class:lit=move || lit(Pick::Group)
                                title="Its group: drag onto a group, or click and pick one"
                                aria-label="Its group: drag onto a group, or click and pick one"
                                aria-pressed=move || lit(Pick::Group).to_string()
                                on:pointerdown=move |ev| down(Grab::Move, ev)
                                on:pointermove=moved
                                on:pointerup=up
                                on:pointercancel=cancelled
                            ></button>
                        })}
                        <Show when=move || days().is_some()>
                            <div class="draftcard-days" class:held=move || held.with(|h| h.is_some_and(|h| h.grab == Grab::Days))>
                                <Show
                                    when=move || typing.get()
                                    fallback=move || view! {
                                        <button
                                            type="button"
                                            class="draftcard-daycount"
                                            title="Its days: drag sideways, or click to type"
                                            aria-label="Its days: drag sideways, or click to type"
                                            on:pointerdown=move |ev| down(Grab::Days, ev)
                                            on:pointermove=moved
                                            on:pointerup=up
                                            on:pointercancel=cancelled
                                        >
                                            <span>{move || format!("{}d", days().unwrap_or(0))}</span>
                                            <i></i>
                                        </button>
                                        // while it is dragged, a box a day: the ones added lit, the ones taken away empty
                                        {move || held.get().filter(|h| h.grab == Grab::Days && h.dragged()).map(|h| {
                                            let now = days().unwrap_or(h.days);
                                            let boxes = (0..now.max(h.days).min(MOST_BOXES)).map(|i| {
                                                let class = match (i < h.days, i < now) {
                                                    (true, true) => "",
                                                    (false, _) => "added",
                                                    (true, false) => "taken",
                                                };
                                                view! { <i class=class></i> }
                                            }).collect_view();
                                            view! { <span class="draftcard-drag">{boxes}</span> }
                                        })}
                                    }
                                >
                                    <input
                                        type="number"
                                        class="draftcard-daybox"
                                        node_ref=days_box
                                        min=move || node.get().map_or(0, least_days)
                                        max=MOST_DAYS
                                        // empty to type into, with the days it has shown behind
                                        placeholder=move || days().unwrap_or(0).to_string()
                                        on:blur=move |_| typed()
                                        on:keydown=move |ev| match ev.key().as_str() {
                                            "Enter" => { ev.prevent_default(); ev.stop_propagation(); typed(); }
                                            "Escape" => { ev.stop_propagation(); typing.set(false); }
                                            _ => {}
                                        }
                                    />
                                </Show>
                            </div>
                        </Show>
                    </div>
                    {knob(Pick::After, "What waits for it: drag onto an item, or click and pick items")}
                </div>
                {move || said().map(|said| view! { <p class="draftcard-said">{said}</p> })}
            </div>
            {line}
        </Show>
    }
}
// }}}

// picked {{{
/// What a click on the chart does while a handle of the card is lit: the item clicked is linked
/// or unlinked, or the item being edited goes in its group. `None` for a click on nothing.
/// Gives whether the click was the card's, which it is whenever a handle is lit.
pub fn picked(plan: &Plan, draft: &mut Draft, node: Option<PlanNode>) -> Result<bool, String> {
    let Some(pick) = draft.pick else {
        return Ok(false);
    };
    match (pick, node) {
        (Pick::Group, over) => draft.place(plan, over)?,
        (_, Some(other)) => draft.toggle(plan, pick, other)?,
        (_, None) => {}
    }
    Ok(true)
}

/// The group of the item a draft is for when it is added from the chart: the group that is
/// open, or the group of the item that is
pub fn beside(plan: &Plan, open: Option<PlanNode>) -> Parent {
    match open {
        Some(PlanNode::TaskGroup(id)) => Parent::Node(id),
        Some(other) => model::parent_of(plan, other),
        None => Parent::Plan,
    }
}
// }}}
