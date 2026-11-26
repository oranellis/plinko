//! Editing one item of a plan. Each editor keeps a draft of its fields and sends only what was
//! changed, so two people editing different fields of an item do not undo each other.

// modules {{{
use std::collections::HashSet;

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, dates::NodeEnd, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        milestone::{Milestone, MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelay, PlanDelayPatch},
        task::{Assignee, NewTask, PersistentState, Status, Task, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroup, TaskGroupPatch},
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
    protocol::Created,
};
use uuid::Uuid;

use crate::{
    datepicker::{DatePicker, OptionalDate},
    draft::{Draft, Drafting, NEW_DAYS},
    format, forms,
    icons::{Icon, IconKind},
    model,
    sync::Sync,
    ui::{Actions, Block, Choice, Field, Hours, List, Picker, Segmented, Text, options, use_sync},
};
// }}}

// helpers {{{
/// What a plan holds right now, or `None` if it is gone
fn read<R>(plan: ShortId, f: impl FnOnce(&Plan) -> Option<R>) -> Option<R> {
    let sync = use_sync();
    sync.tenant.with(|t| f(t.as_ref()?.plans().get(&plan)?))
}

/// `read` for starting values, which are taken once and not followed
fn snapshot<R>(plan: ShortId, f: impl FnOnce(&Plan) -> Option<R>) -> Option<R> {
    let sync = use_sync();
    sync.tenant
        .with_untracked(|t| f(t.as_ref()?.plans().get(&plan)?))
}

fn changed<T: PartialEq>(old: &T, new: T) -> Option<T> {
    (*old != new).then_some(new)
}

/// Whether the form is for an item that is not in the plan yet
fn is_new() -> bool {
    let draft = expect_context::<Drafting>().0;
    draft.with_untracked(|d| d.as_ref().is_some_and(|d| d.new))
}

/// The item a form is for: the one in the plan, or for a new one what adding it would give
fn item<T>(
    plan: ShortId,
    found: impl FnOnce(&Plan) -> Option<T>,
    made: impl FnOnce(Parent) -> T,
) -> Option<T> {
    let draft = expect_context::<Drafting>().0;
    match draft.with_untracked(|d| d.as_ref().filter(|d| d.new).map(|d| d.parent)) {
        Some(parent) => Some(made(parent)),
        None => snapshot(plan, found),
    }
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

/// Saves a form: its patch, which has the place, links and days of the draft in it, and what the
/// draft says of the items that wait for this one. A new item is added first, and the rest then
/// goes to the id it was given.
fn commit(sync: Sync, plan: ShortId, draft: &Draft, name: String, patch: PlanUpdate) {
    let (draft, node) = (draft.clone(), draft.node);
    let adding = draft.new.then(|| draft.adding(name));
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

// ItemEditor {{{
/// The form for one item, to go in a window. `on_close` is for after it is saved or deleted.
#[component]
pub fn ItemEditor(
    plan: ShortId,
    node: PlanNode,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    // there is nothing to take out of the plan for an item that is not in it yet
    let new = is_new();
    let remove = Callback::new(move |()| match new {
        true => on_cancel.run(()),
        false => {
            remove_item(sync, plan, node);
            on_close.run(());
        }
    });
    view! {
        <div class="editor">
            <Info plan=plan node=node />
            {match node {
                PlanNode::Task(id) => view! { <TaskForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::Milestone(id) => view! { <MilestoneForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::PlanBuffer(id) => view! { <BufferForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::PlanDelay(id) => view! { <DelayForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::TaskGroup(id) => view! { <GroupForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
            }}
        </div>
    }
}
// }}}

// Info {{{
/// Where the scheduler put the item, and why it could not
#[component]
fn Info(plan: ShortId, node: PlanNode) -> impl IntoView {
    let sync = use_sync();
    let task_id = match node {
        PlanNode::Task(id) => Some(id),
        _ => None,
    };
    let lines = move || {
        let schedule = sync.schedule.get();
        let state = schedule.as_ref().and_then(|(_, s)| s.plans.get(&plan));
        let mut lines: Vec<String> = Vec::new();
        let Some(state) = state else {
            return lines;
        };
        if let Some(reason) = state.unscheduled.get(&node) {
            lines.push(format!("Not scheduled: {}", model::reason(*reason)));
        }
        if let Some(v) = state.constraint_violations.get(&node) {
            lines.push(format!(
                "Starts {}, which misses its date",
                format::day(v.scheduled)
            ));
        }
        if let Some(task) = task_id.and_then(|id| state.tasks.get(&id)) {
            let range = task.date_range;
            let last = range.end().pred_opt().unwrap_or(range.start());
            lines.push(format!(
                "{} to {}",
                format::day(range.start()),
                format::day(last)
            ));
            for (who, days) in &task.assignments {
                let hours: u32 = days.values().map(|h| u32::from(h.get())).sum();
                lines.push(format!(
                    "{}: {hours} hours over {} days",
                    who_name(*who),
                    days.len()
                ));
            }
        }
        lines
    };
    view! {
        <ul class="info">
            {move || lines().into_iter().map(|l| view! { <li>{l}</li> }).collect_view()}
        </ul>
    }
}
// }}}

// who_name {{{
fn who_name(who: Assignee) -> String {
    let sync = use_sync();
    match who {
        Assignee::Placeholder => "Placeholder".to_string(),
        Assignee::User(id) => sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.users().get(&id))
                .map_or("Someone".to_string(), |u| u.name().to_string())
        }),
    }
}
// }}}

// pickers {{{
/// How the item is held to a date, and the date for the kinds that have one. With `end` the
/// date can be for the first or the last day of the item.
#[component]
fn Constraint(
    kind: RwSignal<String>,
    day: RwSignal<NaiveDate>,
    #[prop(optional)] end: Option<RwSignal<NodeEnd>>,
) -> impl IntoView {
    let which = end.map(|end| {
        view! {
            <Block label="Constraint Applies To">
                <Segmented
                    value=Signal::derive(move || forms::end_value(end.get()).to_string())
                    options=options([("start", "First Day"), ("end", "Last Day")])
                    on_change=Callback::new(move |v: String| end.set(forms::parse_end(&v)))
                />
            </Block>
        }
    });
    view! {
        <div class="cols">
            <Block label="Constraint Type">
                <Segmented
                    value=kind
                    options=options([
                        ("none", "None"),
                        ("earliest", "Earliest"),
                        ("fixed", "Fixed"),
                        ("latest", "Latest"),
                    ])
                    on_change=Callback::new(move |v| kind.set(v))
                />
            </Block>
            <Field label="Constraint Date">
                // clearing the date takes the constraint away
                <DatePicker
                    value=Signal::derive(move || (kind.get() != "none").then(|| day.get()))
                    on_change=Callback::new(move |d: Option<NaiveDate>| match d {
                        Some(d) => day.set(d),
                        None => kind.set("none".to_string()),
                    })
                    clearable=true
                    disabled=Signal::derive(move || kind.get() == "none")
                />
            </Field>
        </div>
        {which}
    }
}

/// The people and groups work can be asked of in this plan
fn worker_options(plan: ShortId) -> Signal<Vec<(String, String)>> {
    let sync = use_sync();
    Signal::derive(move || {
        let mut options = vec![("placeholder".to_string(), "Placeholder".to_string())];
        let names = sync.tenant.with(|t| {
            let t = t.as_ref()?;
            let p = t.plans().get(&plan)?;
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
            Some(named)
        });
        options.extend(names.unwrap_or_default());
        options
    })
}

#[component]
fn Workers(plan: ShortId, allocations: RwSignal<Vec<(Worker, u8)>>) -> impl IntoView {
    let named = worker_options(plan);
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
                // the first person, tag or group not yet asked for, which the row's picker changes
                let me = use_sync().me.get_untracked().map(|m| Worker::User(m.id));
                let taken = move |w: &Worker| allocations.with_untracked(|a| a.iter().any(|(x, _)| x == w));
                let others = named
                    .get_untracked()
                    .into_iter()
                    .filter_map(|(v, _)| forms::parse_worker(&v))
                    .filter(|w| *w != Worker::Placeholder);
                // yourself first when you are in the plan, then whoever is next
                let people: Vec<Worker> = others.collect();
                let next = me
                    .filter(|m| people.contains(m))
                    .into_iter()
                    .chain(people.iter().copied())
                    .find(|w| !taken(w));
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
                view! {
                    <div class="row">
                        <Picker
                            value=worker
                            choices=choices
                            placeholder="Select worker…"
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
// }}}

// forms {{{
#[component]
fn TaskForm(
    plan: ShortId,
    id: Uuid,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
    remove: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let found = |p: &Plan| p.tasks().get(&id).cloned();
    let made = |parent| {
        Task::from(NewTask {
            id,
            name: "New task".to_string(),
            parent,
        })
    };
    let Some(task) = item(plan, found, made) else {
        return view! { <p>"This task has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let name = RwSignal::new(task.name().to_string());
    let constraint = task.time_constraint();
    let (constraint_kind, constraint_day) = forms::constraint_parts(constraint.constraint);
    let kind = RwSignal::new(constraint_kind.to_string());
    let day = RwSignal::new(constraint_day.unwrap_or(today));
    let end = RwSignal::new(constraint.end);
    let mut asked: Vec<(Worker, u8)> = task
        .allocations()
        .iter()
        .map(|(w, h)| (*w, h.get()))
        .collect();
    asked.sort_by_key(|(w, _)| forms::worker_value(*w));
    // whoever adds a task does it for half a day, unless they say otherwise
    let me = sync.me.get_untracked().map(|m| (Worker::User(m.id), 4));
    asked.extend(me.filter(|_| is_new()));
    let allocations = RwSignal::new(asked);
    let affinities = RwSignal::new(task.affinities().clone());
    let state = task.persistent_state();
    // the status is set from the bar under the selected item, here it only says which days apply
    let status = state.status;
    let started = RwSignal::new(state.actual_start_date);
    let ended = RwSignal::new(state.actual_end_date);
    let recorded = RwSignal::new(task.actual_workers().clone());
    let original = StoredValue::new(task);

    let drafting = expect_context::<Drafting>().0;
    let save = move |_| {
        let Some(draft) = drafting.get_untracked() else {
            return;
        };
        let old: Task = original.get_value();
        let allocations: std::collections::HashMap<Worker, Workload> = allocations
            .get_untracked()
            .into_iter()
            .filter_map(|(w, h)| Some((w, Workload::new(h).ok()?)))
            .collect();
        let new_state = PersistentState {
            status,
            actual_start_date: started.get_untracked(),
            actual_end_date: ended.get_untracked(),
        };
        let kept: std::collections::HashMap<Worker, Assignee> = recorded
            .get_untracked()
            .into_iter()
            .filter(|(worker, _)| allocations.contains_key(worker))
            .collect();
        let patch = TaskPatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            parent: changed(&old.parent(), draft.parent),
            duration: changed(&old.duration(), draft.duration.unwrap_or(1).clamp(1, 3650)),
            time_constraint: changed(
                &old.time_constraint(),
                forms::node_constraint(
                    &kind.get_untracked(),
                    day.get_untracked(),
                    end.get_untracked(),
                ),
            ),
            dependencies: changed(old.dependencies(), draft.dependencies.clone()),
            allocations: changed(old.allocations(), allocations),
            affinities: changed(old.affinities(), affinities.get_untracked()),
            // who did the work is whoever was asked for, so what is kept is only for those still asked
            actual_workers: changed(old.actual_workers(), kept),
            persistent_state: changed(&old.persistent_state(), new_state),
        };
        commit(
            sync,
            plan,
            &draft,
            name.get_untracked(),
            PlanUpdate::Task(Crud::Update(patch)),
        );
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Constraint kind=kind day=day end=end />
            <div class="cols">
                <Field label=Signal::derive(move || {
                    let missing = matches!(status, Status::InProgress | Status::OnHold) && started.get().is_none();
                    if missing { "Actual Start *" } else { "Actual Start" }.to_string()
                })>
                    <OptionalDate value=started disabled=Signal::derive(move || status == Status::NotStarted) />
                </Field>
                <Field label=Signal::derive(move || {
                    let missing = matches!(status, Status::Completed | Status::Dropped) && ended.get().is_none();
                    let label = if status == Status::OnHold { "Paused On" } else { "Actual End" };
                    if missing { format!("{label} *") } else { label.to_string() }
                })>
                    <OptionalDate value=ended disabled=Signal::derive(move || matches!(status, Status::NotStarted | Status::InProgress)) />
                </Field>
            </div>
            <Workers plan=plan allocations=allocations />
            <Affinities plan=plan chosen=affinities />
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
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

#[component]
fn MilestoneForm(
    plan: ShortId,
    id: Uuid,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
    remove: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let found = |p: &Plan| p.milestones().get(&id).cloned();
    let made = |parent| {
        Milestone::from(NewMilestone {
            id,
            name: "New milestone".to_string(),
            parent,
        })
    };
    let Some(milestone) = item(plan, found, made) else {
        return view! { <p>"This milestone has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let name = RwSignal::new(milestone.name().to_string());
    let (constraint_kind, constraint_day) = forms::constraint_parts(milestone.time_constraint());
    let kind = RwSignal::new(constraint_kind.to_string());
    let day = RwSignal::new(constraint_day.unwrap_or(today));
    let original = StoredValue::new(milestone);
    let drafting = expect_context::<Drafting>().0;
    let save = move |_| {
        let Some(draft) = drafting.get_untracked() else {
            return;
        };
        let old = original.get_value();
        let patch = MilestonePatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            parent: changed(&old.parent(), draft.parent),
            time_constraint: changed(
                &old.time_constraint(),
                forms::constraint_from(&kind.get_untracked(), day.get_untracked()),
            ),
            dependencies: changed(old.dependencies(), draft.dependencies.clone()),
            persistent_state: None,
        };
        commit(
            sync,
            plan,
            &draft,
            name.get_untracked(),
            PlanUpdate::Milestone(Crud::Update(patch)),
        );
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Constraint kind=kind day=day />
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
}

#[component]
fn BufferForm(
    plan: ShortId,
    id: Uuid,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
    remove: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let found = |p: &Plan| p.plan_buffers().get(&id).cloned();
    let made = |parent| {
        PlanBuffer::from(NewPlanBuffer {
            id,
            parent,
            duration: NEW_DAYS,
        })
    };
    let Some(buffer) = item(plan, found, made) else {
        return view! { <p>"This buffer has been removed."</p> }.into_any();
    };
    let node = PlanNode::PlanBuffer(id);
    let set = RwSignal::new(*buffer.set_start_day());
    let original = StoredValue::new(buffer);
    let drafting = expect_context::<Drafting>().0;
    let save = move |_| {
        let Some(draft) = drafting.get_untracked() else {
            return;
        };
        let old = original.get_value();
        let patch = PlanBufferPatch {
            id,
            parent: changed(&old.parent(), draft.parent),
            dependencies: changed(old.dependencies(), draft.dependencies.clone()),
            set_start_day: changed(old.set_start_day(), set.get_untracked()),
            duration: changed(old.duration(), draft.duration.unwrap_or(0).min(3650)),
        };
        commit(
            sync,
            plan,
            &draft,
            String::new(),
            PlanUpdate::PlanBuffer(Crud::Update(patch)),
        );
        on_close.run(());
    };
    // whether the set start is the day the buffer is scheduled to start on
    let here = move || {
        let state = sync
            .schedule
            .with(|s| s.as_ref().and_then(|(_, s)| s.plans.get(&plan)).cloned());
        let scheduled = model::span_of(state.as_ref(), node).map(|(start, _)| start);
        set.get().is_some() && set.get() == scheduled
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Set Start (it grows or shrinks to keep its end)">
                <div class="row">
                    <OptionalDate value=set />
                    <button
                        type="button"
                        title="Set the start to the day the buffer is scheduled to start on"
                        class="setbtn"
                        class:done=here
                        on:click=move |_| {
                            let state = sync.schedule.with_untracked(|s| {
                                s.as_ref().and_then(|(_, s)| s.plans.get(&plan)).cloned()
                            });
                            if let Some((start, _)) = model::span_of(state.as_ref(), node) {
                                set.set(Some(start));
                            }
                        }
                    >
                        {move || if here() {
                            view! { "Set " <Icon kind=IconKind::Tick size=14 /> }.into_any()
                        } else if set.with(Option::is_some) {
                            "Reset?".into_any()
                        } else {
                            "Set Here?".into_any()
                        }}
                    </button>
                </div>
            </Field>
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
}

#[component]
fn DelayForm(
    plan: ShortId,
    id: Uuid,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
    remove: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let found = |p: &Plan| p.plan_delays().get(&id).cloned();
    let made = |parent| {
        PlanDelay::from(NewPlanDelay {
            id,
            name: None,
            parent,
            duration: NEW_DAYS,
        })
    };
    let Some(delay) = item(plan, found, made) else {
        return view! { <p>"This delay has been removed."</p> }.into_any();
    };
    let name = RwSignal::new(delay.name().unwrap_or_default().to_string());
    let original = StoredValue::new(delay);
    let drafting = expect_context::<Drafting>().0;
    let save = move |_| {
        let Some(draft) = drafting.get_untracked() else {
            return;
        };
        let old = original.get_value();
        // a delay with no name is shown by how long it is
        let named = Some(name.get_untracked()).filter(|n| !n.trim().is_empty());
        let patch = PlanDelayPatch {
            id,
            name: changed(&old.name().map(str::to_string), named),
            parent: changed(&old.parent(), draft.parent),
            dependencies: changed(old.dependencies(), draft.dependencies.clone()),
            duration: changed(old.duration(), draft.duration.unwrap_or(0).min(3650)),
        };
        commit(
            sync,
            plan,
            &draft,
            name.get_untracked(),
            PlanUpdate::PlanDelay(Crud::Update(patch)),
        );
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name (optional)">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
}

#[component]
fn GroupForm(
    plan: ShortId,
    id: Uuid,
    on_close: Callback<()>,
    on_cancel: Callback<()>,
    remove: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let found = |p: &Plan| p.task_groups().get(&id).cloned();
    let made = |parent| {
        TaskGroup::from(NewTaskGroup {
            id,
            name: "New group".to_string(),
            parent,
        })
    };
    let Some(group) = item(plan, found, made) else {
        return view! { <p>"This group has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let name = RwSignal::new(group.name().to_string());
    let constraint = group.time_constraint();
    let (constraint_kind, constraint_day) = forms::constraint_parts(constraint.constraint);
    let kind = RwSignal::new(constraint_kind.to_string());
    let day = RwSignal::new(constraint_day.unwrap_or(today));
    let end = RwSignal::new(constraint.end);
    let original = StoredValue::new(group);
    let drafting = expect_context::<Drafting>().0;
    let save = move |_| {
        let Some(draft) = drafting.get_untracked() else {
            return;
        };
        let old = original.get_value();
        let patch = TaskGroupPatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            dependencies: changed(old.dependencies(), draft.dependencies.clone()),
            parent: changed(&old.parent(), draft.parent),
            time_constraint: changed(
                &old.time_constraint(),
                forms::node_constraint(
                    &kind.get_untracked(),
                    day.get_untracked(),
                    end.get_untracked(),
                ),
            ),
        };
        commit(
            sync,
            plan,
            &draft,
            name.get_untracked(),
            PlanUpdate::TaskGroup(Crud::Update(patch)),
        );
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Constraint kind=kind day=day end=end />
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
}
// }}}
