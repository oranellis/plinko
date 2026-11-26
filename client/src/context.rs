//! The bar of buttons under the item selected on the chart: its status, its data and editing it.
//! While the item is edited the bar is at the bottom of the chart with the item's name, who works
//! on it and saving, leaving and deleting it, which the card of a new item has too.

// modules {{{
use leptos::{html, prelude::*};
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        parent::Parent,
        plan_buffer::PlanBufferPatch,
        task::{Status, Worker},
    },
    planner::plan_update::PlanUpdate,
};
use uuid::Uuid;

use crate::{
    card::{Card, Probe},
    data::{blocks_said, has_data},
    draft::Draft,
    editor::WorkersWindow,
    format,
    gantt::shift::Held,
    icons::{Icon, IconKind},
    mine::set_status,
    model,
    ui::use_sync,
};
// }}}

// constants {{{
/// Every status in the order the column lists them, with its icon and the colour it lights up in
const STATUSES: [(Status, &str, IconKind, &str); 5] = [
    (
        Status::NotStarted,
        "Not started",
        IconKind::Circle,
        "#9e9e9e",
    ),
    (Status::InProgress, "In progress", IconKind::Play, "#ff9800"),
    (Status::OnHold, "On hold", IconKind::Pause, "#b39ddb"),
    (Status::Completed, "Completed", IconKind::Tick, "#43c463"),
    (Status::Dropped, "Dropped", IconKind::Drop, "#8d6e63"),
];
// }}}

// helpers {{{
/// Where a click on the status button takes a task: on towards done
fn next(status: Status) -> Status {
    match status {
        Status::NotStarted | Status::OnHold => Status::InProgress,
        Status::InProgress | Status::Completed => Status::Completed,
        Status::Dropped => Status::NotStarted,
    }
}

fn position(status: Status) -> usize {
    STATUSES.iter().position(|s| s.0 == status).unwrap_or(0)
}
// }}}

// ContextBar {{{
/// The bar of an item of the plan, put where `at` says on the chart. While its item is edited,
/// or for an item that is being created, it is `docked`: at the bottom of the chart, with the
/// shape of the item over the buttons, beside the `InfoPanel`.
#[component]
pub fn ContextBar(
    plan: ShortId,
    /// The item of the plan the bar is for, which a new item is not yet
    node: Option<PlanNode>,
    at: Memo<Option<(f64, f64)>>,
    docked: Memo<bool>,
    draft: RwSignal<Option<Draft>>,
    probe: Probe,
    spot: RwSignal<Option<PlanNode>>,
    on_change: Callback<()>,
    on_edit: Callback<()>,
    on_data: Callback<()>,
    on_save: Callback<()>,
    on_cancel: Callback<()>,
    on_delete: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            class="contextbar"
            class:docked=move || docked.get()
            // nothing waits for it: a little blue in its top right corner
            class:leaf=move || docked.get() && draft.with(|d| d.as_ref().is_some_and(|d| d.dependents.is_empty()))
            class:fadein=node.is_none()
            style=move || match (docked.get(), at.get()) {
                (false, Some((left, top))) => format!("left:{left}px;top:{top}px"),
                _ => String::new(),
            }
            on:click=|ev| ev.stop_propagation()
        >
            <Show when=move || docked.get()>
                <Card plan=plan draft=draft probe=probe spot=spot on_change=on_change />
            </Show>
            <div class="stripbuttons">
            {match node {
                Some(PlanNode::Task(id)) => view! { <StatusSwitch plan=plan task=id /> }.into_any(),
                Some(PlanNode::PlanBuffer(id)) => view! { <SetStart plan=plan buffer=id /> }.into_any(),
                _ => ().into_any(),
            }}
            {node.is_some_and(has_data).then(|| view! {
                <button type="button" class="ctx" title="Data" aria-label="Data" on:click=move |_| on_data.run(())>
                    <Icon kind=IconKind::Data size=20 />
                </button>
            })}
            // the pencil starts the edit, and is then what ends it
            <Show
                when=move || docked.get()
                fallback=move || view! {
                    <button type="button" class="ctx" title="Edit" aria-label="Edit" on:click=move |_| on_edit.run(())>
                        <Icon kind=IconKind::Pencil size=20 />
                    </button>
                }
            >
                <Extras plan=plan draft=draft />
                {match node {
                    Some(_) => view! { <EditSwitch on_save=on_save on_cancel=on_cancel on_delete=on_delete /> }.into_any(),
                    None => view! { <EditSwitch on_save=on_save on_cancel=on_cancel /> }.into_any(),
                }}
            </Show>
            </div>
        </div>
    }
}
// }}}

// InfoPanel {{{
/// What there is to say of an item: its name over its facts. It is in the corner of the chart
/// for the item that is looked at, and slides up beside the docked bar for the one being
/// created or edited, whose name is then typed over where it stands.
#[component]
pub fn InfoPanel(
    plan: ShortId,
    draft: RwSignal<Option<Draft>>,
    #[prop(into)] looked: Signal<Option<PlanNode>>,
    on_save: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let edited = Memo::new(move |_| draft.with(|d| d.as_ref().map(|d| d.node)));
    // what is looked at, as it is in the plan
    let seen = Memo::new(move |_| {
        let node = looked.get().filter(|_| edited.get().is_none())?;
        sync.tenant
            .with(|t| Draft::of(t.as_ref()?.plans().get(&plan)?, node))
    });
    let shown = Signal::derive(move || draft.get().or_else(|| seen.get()));
    let name = move || {
        seen.with(|d| {
            d.as_ref().map(|d| match (d.name.is_empty(), d.node) {
                (false, _) => d.name.clone(),
                (true, PlanNode::PlanBuffer(_)) => "Buffer".to_string(),
                (true, _) => "Delay".to_string(),
            })
        })
    };
    view! {
        <div
            class="infopanel"
            class:docked=move || edited.get().is_some()
            class:gone=move || shown.with(Option::is_none)
            // nothing waits for what is looked at: a little blue in its top right corner
            class:leaf=move || seen.with(|d| d.as_ref().is_some_and(|d| d.dependents.is_empty()))
            on:click=|ev| ev.stop_propagation()
        >
            // made again for each item edited, so the name of a new one is ready to type over
            {move || match edited.get() {
                Some(_) => view! { <NameBox draft=draft on_save=on_save /> }.into_any(),
                None => view! { <p class="infoname">{name}</p> }.into_any(),
            }}
            <Facts plan=plan of=shown />
        </div>
    }
}
// }}}

// NameBox {{{
/// The name of the draft, which is typed over where it stands and takes as many lines as it
/// needs, growing upwards. A buffer has none.
#[component]
pub fn NameBox(draft: RwSignal<Option<Draft>>, on_save: Callback<()>) -> impl IntoView {
    let (node, new) = draft.with_untracked(|d| {
        (
            d.as_ref().map(|d| d.node),
            d.as_ref().is_some_and(|d| d.new),
        )
    });
    let input = NodeRef::<html::Textarea>::new();
    // as high as what is in it
    let fit = move || {
        if let Some(input) = input.try_get_untracked().flatten() {
            let _ = input.set_attribute("style", "height:auto");
            let _ = input.set_attribute("style", &format!("height:{}px", input.scroll_height()));
        }
    };
    // the name of a new item is ready to type over
    set_timeout(
        move || {
            fit();
            if let Some(input) = input.try_get_untracked().flatten().filter(|_| new) {
                let _ = input.focus();
                input.select();
            }
        },
        std::time::Duration::from_millis(80),
    );
    match node {
        None | Some(PlanNode::PlanBuffer(_)) => view! { <span class="namebox fadein">"Buffer"</span> }.into_any(),
        Some(node) => view! {
            <textarea
                class="namebox fadein"
                rows="1"
                node_ref=input
                title="Name"
                aria-label="Name"
                placeholder=if matches!(node, PlanNode::PlanDelay(_)) { "Delay" } else { "Name" }
                prop:value=move || draft.with(|d| d.as_ref().map(|d| d.name.clone()).unwrap_or_default())
                on:input=move |ev| {
                    // a name is one line, however many it is shown on
                    let typed = event_target_value(&ev).replace('\n', " ");
                    draft.update(|d| if let Some(d) = d { d.name = typed });
                    fit();
                }
                on:keydown=move |ev| if ev.key() == "Enter" {
                    ev.prevent_default();
                    on_save.run(());
                }
            ></textarea>
        }
        .into_any(),
    }
}
// }}}

// Facts {{{
/// What there is to say of an item under its name: its group, its days, the day it is held to,
/// who works on it and the data it holds
#[component]
fn Facts(plan: ShortId, of: Signal<Option<Draft>>) -> impl IntoView {
    let sync = use_sync();
    let lines = move || {
        let Some(d) = of.get() else {
            return Vec::new();
        };
        sync.tenant.with(|t| {
            let Some((t, p)) = t.as_ref().and_then(|t| Some((t, t.plans().get(&plan)?))) else {
                return Vec::new();
            };
            let group = match d.parent {
                Parent::Plan => "Top level".to_string(),
                Parent::Node(id) => model::node_name(p, PlanNode::TaskGroup(id)),
            };
            let mut lines = vec![format!("Group: {group}")];
            lines.extend(d.duration.map(|days| match days {
                1 => "Duration: 1 day".to_string(),
                days => format!("Duration: {days} days"),
            }));
            lines.extend(d.pin().map(|pin| {
                let held = match pin.held {
                    Held::Earliest => "Earliest",
                    Held::Latest => "Latest",
                    Held::Fixed => "Fixed",
                };
                // a milestone is one day, with no end to say
                let which = match (d.node, pin.end) {
                    (PlanNode::Milestone(_), _) => "",
                    (_, false) => " start",
                    (_, true) => " end",
                };
                format!("Constraint: {held}{which} {}", format::day(pin.day))
            }));
            let workers: Vec<String> = d
                .allocations
                .iter()
                .map(|(worker, hours)| {
                    let name = match worker {
                        Worker::Placeholder => "Placeholder".to_string(),
                        Worker::User(id) => t
                            .users()
                            .get(id)
                            .map_or("Someone", |u| u.name())
                            .to_string(),
                        Worker::Tag(id) => p
                            .tags()
                            .get(id)
                            .map_or("A tag", |tag| tag.name())
                            .to_string(),
                        Worker::UserGroup(id) => t
                            .user_groups()
                            .get(id)
                            .map_or("A group", |g| g.name())
                            .to_string(),
                    };
                    format!("{name} {hours}h")
                })
                .collect();
            if !workers.is_empty() {
                lines.push(format!("Workers: {}", workers.join(", ")));
            }
            let blocks = blocks_said(p, d.node);
            if !blocks.is_empty() {
                lines.push(format!("Data: {}", blocks.join(", ")));
            }
            lines
        })
    };
    view! {
        <ul class="stripfacts">
            {move || lines().into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
        </ul>
    }
}
// }}}

// Extras {{{
/// Who works on the draft's task, and for an item in the plan which of its ends is held to a day
#[component]
pub fn Extras(plan: ShortId, draft: RwSignal<Option<Draft>>) -> impl IntoView {
    let sync = use_sync();
    let (task, holds) = draft.with_untracked(|d| {
        let node = d.as_ref().map(|d| (d.node, d.new));
        (
            matches!(node, Some((PlanNode::Task(_), _))),
            matches!(
                node,
                Some((
                    PlanNode::Task(_) | PlanNode::TaskGroup(_) | PlanNode::Milestone(_),
                    false
                ))
            ),
        )
    });
    let workers = RwSignal::new(false);
    crate::escape::on_escape(
        move || workers.get(),
        Callback::new(move |()| workers.set(false)),
    );
    // which end is held: its first day, or its last, which is the only one a milestone has
    let held = move || draft.with(|d| d.as_ref().and_then(Draft::pin).map(|p| p.end));
    let single = draft.with_untracked(|d| {
        d.as_ref()
            .is_some_and(|d| matches!(d.node, PlanNode::Milestone(_)))
    });
    let held_label = move || match (held(), single) {
        (None, true) => "Not held to a day: hold it to the day it is on",
        (None, false) => "Not held to a day: hold its first day",
        (Some(false), _) => "Its first day is held: hold its last day instead",
        (Some(true), _) => "Held to a day: let it go",
    };
    // the days the schedule has it on, which is where a day is first held
    let hold_next = move || {
        let today = sync
            .tenant
            .with_untracked(|t| t.as_ref().map(|t| t.today()));
        draft.update(|d| {
            let Some(d) = d else {
                return;
            };
            let span = sync.schedule.with_untracked(|s| {
                let state = s.as_ref().and_then(|(_, s)| s.plans.get(&plan));
                model::span_of(state, d.node)
            });
            let days = span
                .map(|(first, after)| {
                    (
                        first,
                        after.pred_opt().map_or(first, |last| last.max(first)),
                    )
                })
                .or(today.map(|day| (day, day)));
            if let Some((first, last)) = days {
                d.hold_next(first, last);
            }
        });
    };
    view! {
        {task.then(|| view! {
            <button
                type="button"
                class="ctx fadein"
                class:on=move || workers.get()
                title="Workers"
                aria-label="Workers"
                aria-expanded=move || workers.get().to_string()
                on:click=move |_| workers.update(|w| *w = !*w)
            >
                <Icon kind=IconKind::Person size=20 />
            </button>
            <Show when=move || workers.get()>
                <WorkersWindow plan=plan draft=draft />
            </Show>
        })}
        {holds.then(|| view! {
            <button
                type="button"
                class="ctx fadein"
                class:held-start=move || held() == Some(false)
                class:held-end=move || held() == Some(true)
                title=held_label
                aria-label=held_label
                on:click=move |_| hold_next()
            >
                <Icon kind=IconKind::Timer size=20 />
            </button>
        })}
    }
}
// }}}

// EditSwitch {{{
/// What to do with what is being edited, in place of the pencil: a tick that saves it, with a
/// cross that leaves it and a bin that deletes it a hover away, like the statuses of a task
#[component]
pub fn EditSwitch(
    on_save: Callback<()>,
    on_cancel: Callback<()>,
    #[prop(optional, into)] on_delete: Option<Callback<()>>,
) -> impl IntoView {
    let option = |label: &'static str, class: &'static str, kind: IconKind, on: Callback<()>| {
        view! {
            <button type="button" class=format!("st {class}") title=label aria-label=label on:click=move |_| on.run(())>
                <Icon kind=kind size=if kind == IconKind::Bin { 28 } else { 22 } />
            </button>
        }
    };
    view! {
        <div class="statusswitch editswitch fadein">
            <div class="statuscol">
                {option("Save", "next save", IconKind::Tick, on_save)}
                {option("Cancel", "cancel", IconKind::Close, on_cancel)}
                {on_delete.map(|on_delete| option("Delete", "delete", IconKind::Bin, on_delete))}
            </div>
        </div>
    }
}
// }}}

// StatusSwitch {{{
/// One button showing where the task goes next, with the others a hover away
#[component]
fn StatusSwitch(plan: ShortId, task: Uuid) -> impl IntoView {
    let sync = use_sync();
    let status = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let task = t.as_ref()?.plans().get(&plan)?.tasks().get(&task)?;
                Some(task.persistent_state().status)
            })
            .unwrap_or_default()
    });
    view! {
        <div class="statusswitch" style=move || format!("--shift:{}px", -(position(next(status.get())) as i32) * 40)>
            <div class="statuscol">
                {STATUSES
                    .into_iter()
                    .map(|(to, label, icon, colour)| {
                        view! {
                            <button
                                type="button"
                                class="st"
                                class:next=move || to == next(status.get())
                                class:current=move || to == status.get()
                                style=format!("--c:{colour}")
                                title=label
                                aria-label=label
                                on:click=move |_| set_status(plan, task, to)
                            >
                                <Icon kind=icon size=20 />
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
// }}}

// SetStart {{{
/// The buffer's set start: set it to where it is scheduled, set it again, or nothing when it is right
#[component]
fn SetStart(plan: ShortId, buffer: Uuid) -> impl IntoView {
    let sync = use_sync();
    // the set start and the scheduled start
    let days = Memo::new(move |_| {
        let state = sync
            .schedule
            .with(|s| s.as_ref().and_then(|(_, s)| s.plans.get(&plan)).cloned());
        let set = sync.tenant.with(|t| {
            t.as_ref()?
                .plans()
                .get(&plan)?
                .plan_buffers()
                .get(&buffer)
                .map(|b| *b.set_start_day())
        });
        let scheduled = model::span_of(state.as_ref(), PlanNode::PlanBuffer(buffer)).map(|s| s.0);
        (set.flatten(), scheduled)
    });
    let correct = move || {
        let (set, scheduled) = days.get();
        set.is_some() && set == scheduled
    };
    let (kind, label) = (
        move || {
            if correct() {
                IconKind::Tick
            } else if days.get().0.is_some() {
                IconKind::Reset
            } else {
                IconKind::Pin
            }
        },
        move || {
            if correct() {
                "Set to where it is"
            } else if days.get().0.is_some() {
                "Reset to where it is"
            } else {
                "Set to where it is"
            }
        },
    );
    view! {
        <button
            type="button"
            class="ctx"
            title=label
            aria-label=label
            disabled=correct
            on:click=move |_| {
                if let Some(start) = days.get_untracked().1 {
                    let patch = PlanBufferPatch {
                        id: buffer,
                        parent: None,
                        dependencies: None,
                        set_start_day: Some(Some(start)),
                        duration: None,
                    };
                    sync.plan(plan, PlanUpdate::PlanBuffer(Crud::Update(patch)));
                }
            }
        >
            {move || view! { <Icon kind=kind() size=20 /> }}
        </button>
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_moves_a_task_on_towards_done() {
        assert_eq!(next(Status::NotStarted), Status::InProgress);
        assert_eq!(next(Status::InProgress), Status::Completed);
        assert_eq!(next(Status::OnHold), Status::InProgress);
        assert_eq!(next(Status::Dropped), Status::NotStarted);
        assert_eq!(next(Status::Completed), Status::Completed);
    }

    #[test]
    fn the_column_lists_statuses_top_down() {
        let order: Vec<Status> = STATUSES.iter().map(|s| s.0).collect();
        assert_eq!(
            order,
            [
                Status::NotStarted,
                Status::InProgress,
                Status::OnHold,
                Status::Completed,
                Status::Dropped
            ]
        );
        assert_eq!(position(Status::OnHold), 2);
    }
}
// }}}
