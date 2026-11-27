//! Saving the item being created or edited: only what was changed is sent, so two people
//! editing different fields of an item do not undo each other. And the window for who works on
//! a task.

// modules {{{
use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        milestone::{Milestone, MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelay, PlanDelayPatch},
        task::{Assignee, NewTask, Task, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroup, TaskGroupPatch},
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
    protocol::Created,
};
use uuid::Uuid;

use crate::{
    draft::{Draft, NEW_DAYS},
    forms,
    icons::IconKind,
    sync::Sync,
    ui::{Block, Choice, Hours, List, Picker, use_sync},
};
// }}}

// helpers {{{
/// What a plan holds right now, or `None` if it is gone
fn read<R>(plan: ShortId, f: impl FnOnce(&Plan) -> Option<R>) -> Option<R> {
    let sync = use_sync();
    sync.tenant.with(|t| f(t.as_ref()?.plans().get(&plan)?))
}

fn changed<T: PartialEq>(old: &T, new: T) -> Option<T> {
    (*old != new).then_some(new)
}

/// An update to an item, for the item with another id
fn retarget(update: PlanUpdate, id: Uuid) -> PlanUpdate {
    match update {
        PlanUpdate::Task(Crud::Update(p)) => PlanUpdate::Task(Crud::Update(TaskPatch { id, ..p })),
        PlanUpdate::Milestone(Crud::Update(p)) => {
            PlanUpdate::Milestone(Crud::Update(MilestonePatch { id, ..p }))
        }
        PlanUpdate::PlanBuffer(Crud::Update(p)) => {
            PlanUpdate::PlanBuffer(Crud::Update(PlanBufferPatch { id, ..p }))
        }
        PlanUpdate::PlanDelay(Crud::Update(p)) => {
            PlanUpdate::PlanDelay(Crud::Update(PlanDelayPatch { id, ..p }))
        }
        PlanUpdate::TaskGroup(Crud::Update(p)) => {
            PlanUpdate::TaskGroup(Crud::Update(TaskGroupPatch { id, ..p }))
        }
        other => other,
    }
}

/// What a draft changes of its item: the one in the plan, or for a new one what adding it gives.
/// What waits for the item is not the item's to say, see `Draft::dependent_updates`.
fn patch(plan: &Plan, draft: &Draft) -> Option<PlanUpdate> {
    let (id, parent, name) = (draft.node.id(), draft.parent, draft.name.clone());
    let dependencies = draft.dependencies.clone();
    let days = |least: u32| draft.duration.unwrap_or(least).clamp(least, 3650);
    Some(match draft.node {
        PlanNode::Task(_) => {
            let old = match draft.new {
                true => Task::from(NewTask { id, name, parent }),
                false => plan.tasks().get(&id)?.clone(),
            };
            let allocations: HashMap<Worker, Workload> = draft
                .allocations
                .iter()
                .filter_map(|(w, h)| Some((*w, Workload::new(*h).ok()?)))
                .collect();
            // who did the work is whoever was asked for, so what is kept is only for those still asked
            let kept: HashMap<Worker, Assignee> = old
                .actual_workers()
                .iter()
                .filter(|(worker, _)| allocations.contains_key(worker))
                .map(|(w, a)| (*w, *a))
                .collect();
            let state = old.persistent_state();
            PlanUpdate::Task(Crud::Update(TaskPatch {
                id,
                name: changed(&old.name().to_string(), draft.name.clone()),
                parent: changed(&old.parent(), parent),
                duration: changed(&old.duration(), days(1)),
                time_constraint: changed(&old.time_constraint(), draft.constraint),
                dependencies: changed(old.dependencies(), dependencies),
                allocations: changed(old.allocations(), allocations),
                affinities: changed(old.affinities(), draft.affinities.clone()),
                actual_workers: changed(old.actual_workers(), kept),
                persistent_state: changed(&state, draft.dates(state)),
            }))
        }
        PlanNode::Milestone(_) => {
            let old = match draft.new {
                true => Milestone::from(NewMilestone { id, name, parent }),
                false => plan.milestones().get(&id)?.clone(),
            };
            PlanUpdate::Milestone(Crud::Update(MilestonePatch {
                id,
                name: changed(&old.name().to_string(), draft.name.clone()),
                parent: changed(&old.parent(), parent),
                time_constraint: changed(&old.time_constraint(), draft.constraint.constraint),
                dependencies: changed(old.dependencies(), dependencies),
                persistent_state: None,
            }))
        }
        PlanNode::TaskGroup(_) => {
            let old = match draft.new {
                true => TaskGroup::from(NewTaskGroup { id, name, parent }),
                false => plan.task_groups().get(&id)?.clone(),
            };
            PlanUpdate::TaskGroup(Crud::Update(TaskGroupPatch {
                id,
                name: changed(&old.name().to_string(), draft.name.clone()),
                dependencies: changed(old.dependencies(), dependencies),
                parent: changed(&old.parent(), parent),
                time_constraint: changed(&old.time_constraint(), draft.constraint),
            }))
        }
        PlanNode::PlanBuffer(_) => {
            let made = NewPlanBuffer {
                id,
                parent,
                duration: NEW_DAYS,
            };
            let old = match draft.new {
                true => PlanBuffer::from(made),
                false => plan.plan_buffers().get(&id)?.clone(),
            };
            PlanUpdate::PlanBuffer(Crud::Update(PlanBufferPatch {
                id,
                parent: changed(&old.parent(), parent),
                dependencies: changed(old.dependencies(), dependencies),
                set_start_day: None,
                duration: changed(old.duration(), days(0)),
            }))
        }
        PlanNode::PlanDelay(_) => {
            // a delay with no name is shown by how long it is
            let named = Some(name).filter(|n| !n.trim().is_empty());
            let made = NewPlanDelay {
                id,
                name: named.clone(),
                parent,
                duration: NEW_DAYS,
            };
            let old = match draft.new {
                true => PlanDelay::from(made),
                false => plan.plan_delays().get(&id)?.clone(),
            };
            PlanUpdate::PlanDelay(Crud::Update(PlanDelayPatch {
                id,
                name: changed(&old.name().map(str::to_string), named),
                parent: changed(&old.parent(), parent),
                dependencies: changed(old.dependencies(), dependencies),
                duration: changed(old.duration(), days(0)),
            }))
        }
    })
}

/// Saves a draft: what it changes of its item, and of the items that wait for it. A new item is
/// added first, and the rest then goes to the id it was given.
pub fn save(sync: Sync, plan: ShortId, draft: &Draft) {
    let found = sync
        .tenant
        .with_untracked(|t| patch(t.as_ref()?.plans().get(&plan)?, draft));
    let Some(patch) = found else {
        return;
    };
    let (draft, node) = (draft.clone(), draft.node);
    let adding = draft.new.then(|| draft.adding());
    let finish = move |node: PlanNode| {
        sync.plan(plan, retarget(patch, node.id()));
        let waiting = sync.tenant.with_untracked(|t| {
            let plan = t.as_ref()?.plans().get(&plan)?;
            Some(draft.dependent_updates(plan, node))
        });
        for update in waiting.unwrap_or_default() {
            sync.plan(plan, update);
        }
    };
    match adding {
        None => finish(node),
        Some(adding) => sync.plan_then(plan, adding, move |created| {
            if let Some(Created::Item(id)) = created {
                finish(node.node_type().node(id));
            }
        }),
    }
}

/// Takes an item out of its plan, joining what was around it back together
pub fn remove_item(sync: Sync, plan: ShortId, node: PlanNode) {
    let updates = sync
        .tenant
        .with_untracked(|t| Some(t.as_ref()?.plans().get(&plan)?.removal(node)))
        .unwrap_or_else(|| vec![PlanUpdate::remove(node)]);
    for update in updates {
        sync.plan(plan, update);
    }
}
// }}}

// WorkersWindow {{{
/// Who works on the task of the draft and for how long a day, and the days it can be worked on:
/// a card at the right of the chart for as long as the task is created or edited
#[component]
pub fn WorkersWindow(
    plan: ShortId,
    draft: RwSignal<Option<Draft>>,
    /// The row the keys are on, while they are on the card
    cursor: RwSignal<Option<usize>>,
    /// A row whose list of who it can be is to open
    open_row: RwSignal<Option<usize>>,
) -> impl IntoView {
    let start = draft.get_untracked();
    let allocations = RwSignal::new(
        start
            .as_ref()
            .map(|d| d.allocations.clone())
            .unwrap_or_default(),
    );
    let affinities = RwSignal::new(start.map(|d| d.affinities).unwrap_or_default());
    // what the keys change of the draft is shown here
    Effect::new(move |_| {
        let asked = draft.with(|d| d.as_ref().map(|d| d.allocations.clone()));
        if let Some(asked) = asked.filter(|a| allocations.with_untracked(|was| was != a)) {
            allocations.set(asked);
        }
    });
    // what is picked here is the draft's
    Effect::new(move |_| {
        let (asked, allowed) = (allocations.get(), affinities.get());
        let same = draft.with_untracked(|d| {
            d.as_ref()
                .is_none_or(|d| d.allocations == asked && d.affinities == allowed)
        });
        if !same {
            draft.update(|d| {
                if let Some(d) = d {
                    (d.allocations, d.affinities) = (asked, allowed);
                }
            });
        }
    });
    view! {
        <div class="workerwin fadein" class:keys=move || cursor.with(Option::is_some) on:click=|ev| ev.stop_propagation()>
            <Workers plan=plan allocations=allocations cursor=cursor open_row=open_row />
            <Affinities plan=plan chosen=affinities />
        </div>
    }
}

/// Who to ask next for work on a task asking `asked`: yourself when you are in the plan, then
/// the first person, tag or group not asked yet
pub fn next_worker(sync: Sync, plan: ShortId, asked: &[(Worker, u8)]) -> Option<Worker> {
    let me = sync.me.get_untracked().map(|m| Worker::User(m.id));
    let options = sync
        .tenant
        .with_untracked(|t| Some(worker_options_of(t.as_ref()?, plan)))
        .unwrap_or_default();
    let people: Vec<Worker> = options
        .into_iter()
        .filter_map(|(v, _)| forms::parse_worker(&v))
        .filter(|w| *w != Worker::Placeholder)
        .collect();
    let taken = |w: &Worker| asked.iter().any(|(x, _)| x == w);
    me.filter(|m| people.contains(m))
        .into_iter()
        .chain(people.iter().copied())
        .find(|w| !taken(w))
}

/// The people and groups work can be asked of in this plan
fn worker_options(plan: ShortId) -> Signal<Vec<(String, String)>> {
    let sync = use_sync();
    Signal::derive(move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| worker_options_of(t, plan)))
            .unwrap_or_default()
    })
}

/// The same from the tenant, read once
fn worker_options_of(
    t: &plinko::multitenancy::tenant::Tenant,
    plan: ShortId,
) -> Vec<(String, String)> {
    let mut options = vec![("placeholder".to_string(), "Placeholder".to_string())];
    let Some(p) = t.plans().get(&plan) else {
        return options;
    };
    let mut named: Vec<(String, String)> = Vec::new();
    for id in p.users().keys() {
        let name = t.users().get(id).map_or("Someone", |u| u.name());
        named.push((forms::worker_value(Worker::User(*id)), name.to_string()));
    }
    for (id, tag) in p.tags() {
        named.push((
            forms::worker_value(Worker::Tag(*id)),
            format!("Anyone tagged {}", tag.name()),
        ));
    }
    for id in p.user_groups().keys() {
        let name = t.user_groups().get(id).map_or("A group", |g| g.name());
        named.push((
            forms::worker_value(Worker::UserGroup(*id)),
            format!("Anyone in {name}"),
        ));
    }
    named.sort_by(|a, b| a.1.cmp(&b.1));
    options.extend(named);
    options
}

#[component]
fn Workers(
    plan: ShortId,
    allocations: RwSignal<Vec<(Worker, u8)>>,
    cursor: RwSignal<Option<usize>>,
    open_row: RwSignal<Option<usize>>,
) -> impl IntoView {
    let named = worker_options(plan);
    let open_person = crate::person::use_open_person();
    let sync = use_sync();
    // a row asked to open has done so once its list has seen it
    Effect::new(move |_| {
        if open_row.get().is_some() {
            set_timeout(
                move || {
                    open_row.try_set(None);
                },
                std::time::Duration::from_millis(50),
            );
        }
    });
    let choices = Signal::derive(move || {
        named
            .get()
            .into_iter()
            .map(|(v, l)| {
                // the placeholder is not a person, so it is shown greyed out
                let tone = if v == "placeholder" { "ghost" } else { "" };
                Choice::new(v, l, tone)
            })
            .collect::<Vec<_>>()
    });
    view! {
        <List
            label="Workers"
            empty="No workers added yet"
            add="Add Worker"
            count=Signal::derive(move || allocations.with(Vec::len))
            on_add=Callback::new(move |()| {
                let next = allocations.with_untracked(|a| next_worker(use_sync(), plan, a));
                if let Some(worker) = next {
                    allocations.update(|a| a.push((worker, 4)));
                }
            })
            extra=(
                IconKind::Placeholder,
                "Add a placeholder, for work nobody is chosen for yet",
                Callback::new(move |()| {
                    allocations.update(|a| {
                        if !a.iter().any(|(w, _)| *w == Worker::Placeholder) {
                            a.push((Worker::Placeholder, 4));
                        }
                    });
                }),
            )
        >
            <For each=move || 0..allocations.with(Vec::len) key=|i| *i children=move |i| {
                let worker = Signal::derive(move || {
                    allocations.with(|a| a.get(i).map(|(w, _)| forms::worker_value(*w)).unwrap_or_default())
                });
                let hours = Signal::derive(move || {
                    allocations.with(|a| a.get(i).map_or(0, |(_, h)| i64::from(*h)))
                });
                // who is asked for on the other rows is not listed again
                let open = Signal::derive(move || {
                    let taken: Vec<String> = allocations.with(|a| {
                        a.iter()
                            .enumerate()
                            .filter(|(j, _)| *j != i)
                            .map(|(_, (w, _))| forms::worker_value(*w))
                            .collect()
                    });
                    choices.with(|all| {
                        all.iter().filter(|c| !taken.contains(&c.value)).cloned().collect::<Vec<_>>()
                    })
                });
                // a person on the row, whose face opens more about them
                let person = Signal::derive(move || {
                    let id = allocations.with(|a| match a.get(i) {
                        Some((Worker::User(id), _)) => Some(*id),
                        _ => None,
                    })?;
                    let name = sync.tenant.with(|t| t.as_ref()?.users().get(&id).map(|u| u.name().to_string()))?;
                    Some((id, name))
                });
                view! {
                    <div class="row" class:cursor=move || cursor.get() == Some(i)>
                        {move || match person.get() {
                            Some((id, name)) => {
                                let title = format!("About {name}");
                                view! {
                                <button type="button" class="facebtn" title=title on:click=move |_| open_person.run(id)>
                                    <crate::person::Face id=id name=name size=26 />
                                </button>
                                }.into_any()
                            }
                            None => view! { <span class="facebtn"></span> }.into_any(),
                        }}
                        <Picker
                            value=worker
                            choices=open
                            placeholder="Select worker…"
                            opener=Signal::derive(move || open_row.get() == Some(i))
                            on_change=Callback::new(move |v: String| {
                                if let Some(w) = forms::parse_worker(&v) {
                                    allocations.update(|a| if let Some(a) = a.get_mut(i) { a.0 = w });
                                }
                            })
                        />
                        <Hours
                            value=hours
                            on_change=Callback::new(move |n: u8| {
                                allocations.update(|a| if let Some(a) = a.get_mut(i) { a.1 = n });
                            })
                        />
                        <button type="button" class="plain" aria-label="Remove worker" on:click=move |_| {
                            allocations.update(|a| { if i < a.len() { a.remove(i); } });
                        }>"✕"</button>
                    </div>
                }
            } />
        </List>
    }
}

#[component]
fn Affinities(plan: ShortId, chosen: RwSignal<HashSet<Uuid>>) -> impl IntoView {
    let all = Signal::derive(move || {
        read(plan, |p| {
            let mut all: Vec<(Uuid, String)> = p
                .affinities()
                .iter()
                .map(|(id, a)| (*id, a.name().to_string()))
                .collect();
            all.sort_by(|a, b| a.1.cmp(&b.1));
            Some(all)
        })
        .unwrap_or_default()
    });
    view! {
        <Show when=move || all.with(|a| !a.is_empty())>
            <Block label="Task Affinity" hint="Limits the task to the days an affinity allows, which are set in the plan settings. With none ticked it can be worked on any day."><div class="list">
                {move || {
                    all.get()
                        .into_iter()
                        .map(|(id, name)| {
                            view! {
                                <label class="check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || chosen.with(|c| c.contains(&id))
                                        on:change=move |ev| {
                                            let on = event_target_checked(&ev);
                                            chosen.update(|c| { if on { c.insert(id); } else { c.remove(&id); } });
                                        }
                                    />
                                    {name}
                                </label>
                            }
                        })
                        .collect_view()
                }}
            </div></Block>
        </Show>
    }
}
// }}}
