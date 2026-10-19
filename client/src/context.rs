//! The bar of buttons under the item selected on the chart: its status, its data and editing it.

// modules {{{
use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::{dependency::PlanNode, plan_buffer::PlanBufferPatch, task::Status},
    planner::plan_update::PlanUpdate,
};
use uuid::Uuid;

use crate::{
    data::has_data,
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
/// The bar, put where `left` and `top` say on the chart
#[component]
pub fn ContextBar(
    plan: ShortId,
    node: PlanNode,
    left: f64,
    top: f64,
    on_edit: Callback<()>,
    on_data: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="contextbar" style=format!("left:{left}px;top:{top}px") on:click=|ev| ev.stop_propagation()>
            {match node {
                PlanNode::Task(id) => view! { <StatusSwitch plan=plan task=id /> }.into_any(),
                PlanNode::PlanBuffer(id) => view! { <SetStart plan=plan buffer=id /> }.into_any(),
                _ => ().into_any(),
            }}
            {has_data(node).then(|| view! {
                <button type="button" class="ctx" title="Data" aria-label="Data" on:click=move |_| on_data.run(())>
                    <Icon kind=IconKind::Data size=18 />
                </button>
            })}
            <button type="button" class="ctx" title="Edit" aria-label="Edit" on:click=move |_| on_edit.run(())>
                <Icon kind=IconKind::Pencil size=18 />
            </button>
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
        <div class="statusswitch" style=move || format!("--shift:{}px", -(position(next(status.get())) as i32) * 34)>
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
                                <Icon kind=icon size=18 />
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
            {move || view! { <Icon kind=kind() size=18 /> }}
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
