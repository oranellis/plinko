//! The bar of buttons under the item selected on the chart: its status, its data and editing it.
//! While the item is edited the bar is at the bottom of the chart with the item's name, who works
//! on it and saving, leaving and deleting it, which the card of a new item has too.

// modules {{{
use leptos::{html, prelude::*};
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::{dependency::PlanNode, plan_buffer::PlanBufferPatch, task::Status},
    planner::plan_update::PlanUpdate,
};
use uuid::Uuid;

use crate::{
    data::has_data,
    draft::Draft,
    editor::WorkersWindow,
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
/// The bar, put where `at` says on the chart, or slid to the middle of its bottom when `docked`
#[component]
pub fn ContextBar(
    plan: ShortId,
    node: PlanNode,
    at: Memo<Option<(f64, f64)>>,
    docked: Memo<bool>,
    draft: RwSignal<Option<Draft>>,
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
            style=move || match (docked.get(), at.get()) {
                (false, Some((left, top))) => format!("left:{left}px;top:{top}px"),
                _ => String::new(),
            }
            on:click=|ev| ev.stop_propagation()
        >
            <Show when=move || docked.get()>
                <NameBox draft=draft on_save=on_save />
            </Show>
            {match node {
                PlanNode::Task(id) => view! { <StatusSwitch plan=plan task=id /> }.into_any(),
                PlanNode::PlanBuffer(id) => view! { <SetStart plan=plan buffer=id /> }.into_any(),
                _ => ().into_any(),
            }}
            {has_data(node).then(|| view! {
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
                <EditSwitch on_save=on_save on_cancel=on_cancel on_delete=on_delete />
                <Extras plan=plan draft=draft />
            </Show>
        </div>
    }
}
// }}}

// NameBox {{{
/// The name of the draft, which is typed over where it stands. A buffer has none.
#[component]
pub fn NameBox(draft: RwSignal<Option<Draft>>, on_save: Callback<()>) -> impl IntoView {
    let (node, new) = draft.with_untracked(|d| {
        (
            d.as_ref().map(|d| d.node),
            d.as_ref().is_some_and(|d| d.new),
        )
    });
    let input = NodeRef::<html::Input>::new();
    // the name of a new item is ready to type over
    if new {
        set_timeout(
            move || {
                if let Some(input) = input.try_get_untracked().flatten() {
                    let _ = input.focus();
                    input.select();
                }
            },
            std::time::Duration::from_millis(80),
        );
    }
    match node {
        None | Some(PlanNode::PlanBuffer(_)) => view! { <span class="namebox fadein">"Buffer"</span> }.into_any(),
        Some(node) => view! {
            <input
                type="text"
                class="namebox fadein"
                node_ref=input
                title="Name"
                aria-label="Name"
                placeholder=if matches!(node, PlanNode::PlanDelay(_)) { "Delay" } else { "Name" }
                prop:value=move || draft.with(|d| d.as_ref().map(|d| d.name.clone()).unwrap_or_default())
                on:input=move |ev| {
                    let typed = event_target_value(&ev);
                    draft.update(|d| if let Some(d) = d { d.name = typed });
                }
                on:keydown=move |ev| if ev.key() == "Enter" {
                    ev.prevent_default();
                    on_save.run(());
                }
            />
        }
        .into_any(),
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
/// What to do with what is being edited, in place of the pencil: one button that saves it, with
/// leaving it and deleting it a hover away, like the statuses of a task
#[component]
pub fn EditSwitch(
    on_save: Callback<()>,
    on_cancel: Callback<()>,
    #[prop(optional, into)] on_delete: Option<Callback<()>>,
) -> impl IntoView {
    // a pencil with what happens to the edit in its corner, or for deleting a bin that fills it
    let option = |label: &'static str,
                  class: &'static str,
                  mark: Option<IconKind>,
                  on: Callback<()>| {
        view! {
            <button type="button" class=format!("st {class}") title=label aria-label=label on:click=move |_| on.run(())>
                {match mark {
                    Some(mark) => view! {
                        <Icon kind=IconKind::Pencil size=20 />
                        <span class="mini"><Icon kind=mark size=12 /></span>
                    }.into_any(),
                    None => view! { <Icon kind=IconKind::Bin size=28 /> }.into_any(),
                }}
            </button>
        }
    };
    view! {
        <div class="statusswitch editswitch fadein">
            <div class="statuscol">
                {option("Save", "next save", Some(IconKind::Tick), on_save)}
                {option("Cancel", "cancel", Some(IconKind::Close), on_cancel)}
                {on_delete.map(|on_delete| option("Delete", "delete", None, on_delete))}
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
