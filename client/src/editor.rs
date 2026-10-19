//! Editing one item of a plan. Each editor keeps a draft of its fields and sends only what was
//! changed, so two people editing different fields of an item do not undo each other.

// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    planner::{plan::Plan, plan_update::PlanUpdate},
    types::{
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::MilestonePatch,
        parent::Parent,
        plan_buffer::PlanBufferPatch,
        short_id::ShortId,
        task::{Assignee, PersistentState, ScheduleMode, Status, Task, TaskPatch, Worker},
        task_group::TaskGroupPatch,
        workload::Workload,
    },
};
use uuid::Uuid;

use crate::{
    datepicker::{DatePicker, OptionalDate},
    format, forms,
    icons::{Icon, IconKind},
    model::{self, node_name},
    sync::Sync,
    ui::{
        Actions, Block, Choice, Field, Hours, List, Number, Picker, Segmented, Select, Text,
        options, use_sync,
    },
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
    let remove = Callback::new(move |()| {
        remove_item(sync, plan, node);
        on_close.run(());
    });
    view! {
        <div class="editor">
            <Info plan=plan node=node />
            {match node {
                PlanNode::Task(id) => view! { <TaskForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::Milestone(id) => view! { <MilestoneForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::PlanBuffer(id) => view! { <BufferForm plan=plan id=id on_close=on_close on_cancel=on_cancel remove=remove /> }.into_any(),
                PlanNode::PlanDelay(_) => ().into_any(),
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
/// Every item of the plan that something can depend on, other than `except`. Plan targets are
/// marked, and the start of the parent is offered when `with_start`.
fn depends_options(plan: ShortId, except: PlanNode, with_start: bool) -> Signal<Vec<Choice>> {
    Signal::derive(move || {
        let mut options = Vec::new();
        if with_start {
            options.push(Choice::new("start", "Parent Start", "start"));
        }
        let named = read(plan, |p| {
            let targets: HashSet<PlanNode> = p.targets().values().map(|t| t.node()).collect();
            let mut named: Vec<Choice> = model::nodes(p)
                .into_iter()
                .filter(|n| *n != except)
                .map(|n| {
                    let tone = if targets.contains(&n) { "target" } else { "" };
                    let value = forms::node_value(DependencyNode::Node(n));
                    Choice::new(value, node_name(p, n), tone)
                })
                .collect();
            named.sort_by(|a, b| a.label.cmp(&b.label).then(a.value.cmp(&b.value)));
            Some(named)
        });
        options.extend(named.unwrap_or_default());
        options
    })
}

/// What an item waits for, or what waits for it, each with a lag in days
#[component]
fn DependencyList(
    #[prop(into)] label: String,
    #[prop(into)] empty: String,
    #[prop(into)] placeholder: String,
    choices: Signal<Vec<Choice>>,
    deps: RwSignal<Vec<Dependency>>,
    /// What a new row starts as
    fresh: Dependency,
) -> impl IntoView {
    view! {
        <List
            label=label
            empty=empty
            add="Add"
            count=Signal::derive(move || deps.with(Vec::len))
            on_add=Callback::new(move |()| deps.update(|d| d.push(fresh.clone())))
        >
            <For each=move || 0..deps.with(Vec::len) key=|i| *i children=move |i| {
                let node = Signal::derive(move || {
                    deps.with(|d| d.get(i).map(|d| forms::node_value(d.node)).unwrap_or_default())
                });
                let lag = Signal::derive(move || deps.with(|d| d.get(i).map_or(0, |d| i64::from(d.lag))));
                view! {
                    <div class="row">
                        <Picker
                            value=node
                            choices=choices
                            placeholder=placeholder.clone()
                            on_change=Callback::new(move |v: String| {
                                if let Some(node) = forms::parse_node(&v) {
                                    deps.update(|d| if let Some(d) = d.get_mut(i) { d.node = node });
                                }
                            })
                        />
                        <Number
                            value=lag
                            on_input=Callback::new(move |n: i64| {
                                let n = n.clamp(-3650, 3650) as i32;
                                deps.update(|d| if let Some(d) = d.get_mut(i) { d.lag = n });
                            })
                        />
                        <span class="small" title="Lag days, positive to delay and negative to start early">"lag"</span>
                        <button type="button" class="plain" aria-label="Remove" on:click=move |_| {
                            deps.update(|d| { if i < d.len() { d.remove(i); } });
                        }>"✕"</button>
                    </div>
                }
            } />
        </List>
    }
}

#[component]
fn Dependencies(plan: ShortId, me: PlanNode, deps: RwSignal<Vec<Dependency>>) -> impl IntoView {
    view! {
        <DependencyList
            label="Dependencies"
            empty="No dependencies added yet"
            placeholder="Select dependency…"
            choices=depends_options(plan, me, true)
            deps=deps
            fresh=Dependency::default()
        />
    }
}

/// The items that wait for this one, which are kept in their own dependencies
#[component]
fn Dependents(plan: ShortId, me: PlanNode, deps: RwSignal<Vec<Dependency>>) -> impl IntoView {
    view! {
        <DependencyList
            label="Required by"
            empty="No dependents added yet"
            placeholder="Select dependent…"
            choices=depends_options(plan, me, false)
            deps=deps
            fresh=Dependency { node: DependencyNode::Node(me), lag: 0 }
        />
    }
}

/// The items that depend on `me`, as dependencies on them
fn dependents_of(plan: &Plan, me: PlanNode) -> Vec<Dependency> {
    let mut found: Vec<(String, Dependency)> = model::nodes(plan)
        .into_iter()
        .filter(|n| *n != me)
        .flat_map(|n| {
            model::dependencies(plan, n)
                .into_iter()
                .filter(|d| d.node == DependencyNode::Node(me))
                .map(move |d| (n, d.lag))
        })
        .map(|(n, lag)| {
            (
                node_name(plan, n),
                Dependency {
                    node: DependencyNode::Node(n),
                    lag,
                },
            )
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, d)| d).collect()
}

fn set_dependencies(sync: Sync, plan: ShortId, node: PlanNode, deps: Vec<Dependency>) {
    sync.plan(plan, PlanUpdate::set_dependencies(node, deps));
}

/// Changes the dependencies of the other items to match what was asked for in `new`
fn apply_dependents(
    sync: Sync,
    plan: ShortId,
    me: PlanNode,
    old: &[Dependency],
    new: &[Dependency],
) {
    let wanted = |deps: &[Dependency]| -> HashMap<PlanNode, i32> {
        deps.iter()
            .filter_map(|d| match d.node {
                DependencyNode::Node(n) if n != me => Some((n, d.lag)),
                _ => None,
            })
            .collect()
    };
    let (old, new) = (wanted(old), wanted(new));
    let affected: HashSet<PlanNode> = old.keys().chain(new.keys()).copied().collect();
    for other in affected {
        if old.get(&other) == new.get(&other) {
            continue;
        }
        let mut deps = sync
            .tenant
            .with_untracked(|t| Some(model::dependencies(t.as_ref()?.plans().get(&plan)?, other)))
            .unwrap_or_default();
        let mine = DependencyNode::Node(me);
        deps.retain(|d| d.node != mine || new.contains_key(&other));
        if let Some(lag) = new.get(&other) {
            match deps.iter_mut().find(|d| d.node == mine) {
                Some(d) => d.lag = *lag,
                None => deps.push(Dependency {
                    node: mine,
                    lag: *lag,
                }),
            }
        }
        set_dependencies(sync, plan, other, deps);
    }
}

#[component]
fn ParentPicker(plan: ShortId, me: PlanNode, parent: RwSignal<Parent>) -> impl IntoView {
    let choices = Signal::derive(move || {
        let mut options = vec![("plan".to_string(), "Top level".to_string())];
        let groups = read(plan, |p| {
            let mut groups: Vec<(String, String)> = p
                .task_groups()
                .values()
                .filter(|g| PlanNode::TaskGroup(*g.id()) != me)
                .map(|g| {
                    (
                        forms::parent_value(Parent::Node(*g.id())),
                        g.name().to_string(),
                    )
                })
                .collect();
            groups.sort();
            Some(groups)
        });
        options.extend(groups.unwrap_or_default());
        options
    });
    view! {
        <Field label="Group">
            <Select
                value=Signal::derive(move || forms::parent_value(parent.get()))
                options=choices
                on_change=Callback::new(move |v: String| {
                    if let Some(p) = forms::parse_parent(&v) {
                        parent.set(p);
                    }
                })
            />
        </Field>
    }
}

/// How the item is held to a date, and the date for the kinds that have one
#[component]
fn Constraint(kind: RwSignal<String>, day: RwSignal<NaiveDate>) -> impl IntoView {
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
    let Some(task) = snapshot(plan, |p| p.tasks().get(&id).cloned()) else {
        return view! { <p>"This task has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let node = PlanNode::Task(id);
    let name = RwSignal::new(task.name().to_string());
    let duration = RwSignal::new(i64::from(task.duration()));
    let mode = RwSignal::new(
        if task.mode() == ScheduleMode::Strict {
            "strict"
        } else {
            "relaxed"
        }
        .to_string(),
    );
    let (constraint_kind, constraint_day) = forms::constraint_parts(task.time_constraint());
    let kind = RwSignal::new(constraint_kind.to_string());
    let day = RwSignal::new(constraint_day.unwrap_or(today));
    let parent = RwSignal::new(task.parent());
    let deps = RwSignal::new(task.dependencies().clone());
    let mut asked: Vec<(Worker, u8)> = task
        .allocations()
        .iter()
        .map(|(w, h)| (*w, h.get()))
        .collect();
    asked.sort_by_key(|(w, _)| forms::worker_value(*w));
    let allocations = RwSignal::new(asked);
    let affinities = RwSignal::new(task.affinities().clone());
    let state = task.persistent_state();
    let status = RwSignal::new(state.status);
    let started = RwSignal::new(state.actual_start_date);
    let ended = RwSignal::new(state.actual_end_date);
    let recorded = RwSignal::new(task.actual_workers().clone());
    let original = StoredValue::new(task);
    let was = snapshot(plan, |p| Some(dependents_of(p, node))).unwrap_or_default();
    let dependents = RwSignal::new(was.clone());

    let save = move |_| {
        let old: Task = original.get_value();
        let allocations: std::collections::HashMap<Worker, Workload> = allocations
            .get_untracked()
            .into_iter()
            .filter_map(|(w, h)| Some((w, Workload::new(h).ok()?)))
            .collect();
        let new_mode = if mode.get_untracked() == "relaxed" {
            ScheduleMode::Relaxed
        } else {
            ScheduleMode::Strict
        };
        let new_state = PersistentState {
            status: status.get_untracked(),
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
            parent: changed(&old.parent(), parent.get_untracked()),
            mode: changed(&old.mode(), new_mode),
            duration: changed(
                &old.duration(),
                duration.get_untracked().clamp(1, 3650) as u32,
            ),
            time_constraint: changed(
                &old.time_constraint(),
                forms::constraint_from(&kind.get_untracked(), day.get_untracked()),
            ),
            dependencies: changed(old.dependencies(), deps.get_untracked()),
            allocations: changed(old.allocations(), allocations),
            affinities: changed(old.affinities(), affinities.get_untracked()),
            // who did the work is whoever was asked for, so what is kept is only for those still asked
            actual_workers: changed(old.actual_workers(), kept),
            persistent_state: changed(&old.persistent_state(), new_state),
        };
        sync.plan(plan, PlanUpdate::Task(Crud::Update(patch)));
        apply_dependents(sync, plan, node, &was, &dependents.get_untracked());
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Block label="Status">
                <Segmented
                    value=Signal::derive(move || forms::status_value(status.get()).to_string())
                    options=options([
                        ("not-started", "Not Started"),
                        ("in-progress", "In Progress"),
                        ("on-hold", "On Hold"),
                        ("completed", "Complete"),
                        ("dropped", "Dropped"),
                    ])
                    on_change=Callback::new(move |v: String| {
                        let Some(next) = forms::parse_status(&v) else { return };
                        status.set(next);
                        match next {
                            Status::NotStarted => { started.set(None); ended.set(None); }
                            _ => {
                                // anything past not started has started, today if it never said
                                if started.get_untracked().is_none() { started.set(Some(today)); }
                                match next {
                                    Status::InProgress => ended.set(None),
                                    // when it was put on hold, or finished or dropped
                                    _ => if ended.get_untracked().is_none() { ended.set(Some(today)); },
                                }
                            }
                        }
                    })
                />
            </Block>
            <div class="cols">
                <Field label="Duration (days)">
                    <Number value=duration on_input=Callback::new(move |n| duration.set(n)) min=1 max=3650 />
                </Field>
                <Field label="Mode">
                    <Select
                        value=mode
                        options=options([("strict", "Strict"), ("relaxed", "Relaxed")])
                        on_change=Callback::new(move |v| mode.set(v))
                    />
                </Field>
            </div>
            <Constraint kind=kind day=day />
            <div class="cols">
                <Field label=Signal::derive(move || {
                    let missing = matches!(status.get(), Status::InProgress | Status::OnHold) && started.get().is_none();
                    if missing { "Actual Start *" } else { "Actual Start" }.to_string()
                })>
                    <OptionalDate value=started disabled=Signal::derive(move || status.get() == Status::NotStarted) />
                </Field>
                <Field label=Signal::derive(move || {
                    let missing = matches!(status.get(), Status::Completed | Status::Dropped) && ended.get().is_none();
                    let label = if status.get() == Status::OnHold { "Paused On" } else { "Actual End" };
                    if missing { format!("{label} *") } else { label.to_string() }
                })>
                    <OptionalDate value=ended disabled=Signal::derive(move || matches!(status.get(), Status::NotStarted | Status::InProgress)) />
                </Field>
            </div>
            <ParentPicker plan=plan me=node parent=parent />
            <Workers plan=plan allocations=allocations />
            <Dependencies plan=plan me=node deps=deps />
            <Dependents plan=plan me=node deps=dependents />
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
    let Some(milestone) = snapshot(plan, |p| p.milestones().get(&id).cloned()) else {
        return view! { <p>"This milestone has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let node = PlanNode::Milestone(id);
    let name = RwSignal::new(milestone.name().to_string());
    let (constraint_kind, constraint_day) = forms::constraint_parts(milestone.time_constraint());
    let kind = RwSignal::new(constraint_kind.to_string());
    let day = RwSignal::new(constraint_day.unwrap_or(today));
    let parent = RwSignal::new(milestone.parent());
    let deps = RwSignal::new(milestone.dependencies().clone());
    let original = StoredValue::new(milestone);
    let was = snapshot(plan, |p| Some(dependents_of(p, node))).unwrap_or_default();
    let dependents = RwSignal::new(was.clone());
    let save = move |_| {
        let old = original.get_value();
        let patch = MilestonePatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            parent: changed(&old.parent(), parent.get_untracked()),
            time_constraint: changed(
                &old.time_constraint(),
                forms::constraint_from(&kind.get_untracked(), day.get_untracked()),
            ),
            dependencies: changed(old.dependencies(), deps.get_untracked()),
            persistent_state: None,
        };
        sync.plan(plan, PlanUpdate::Milestone(Crud::Update(patch)));
        apply_dependents(sync, plan, node, &was, &dependents.get_untracked());
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Constraint kind=kind day=day />
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <Dependents plan=plan me=node deps=dependents />
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
    let Some(buffer) = snapshot(plan, |p| p.plan_buffers().get(&id).cloned()) else {
        return view! { <p>"This buffer has been removed."</p> }.into_any();
    };
    let node = PlanNode::PlanBuffer(id);
    let duration = RwSignal::new(i64::from(*buffer.duration()));
    let set = RwSignal::new(*buffer.set_start_day());
    let parent = RwSignal::new(buffer.parent());
    let deps = RwSignal::new(buffer.dependencies().clone());
    let original = StoredValue::new(buffer);
    let was = snapshot(plan, |p| Some(dependents_of(p, node))).unwrap_or_default();
    let dependents = RwSignal::new(was.clone());
    let save = move |_| {
        let old = original.get_value();
        let patch = PlanBufferPatch {
            id,
            parent: changed(&old.parent(), parent.get_untracked()),
            dependencies: changed(old.dependencies(), deps.get_untracked()),
            set_start_day: changed(old.set_start_day(), set.get_untracked()),
            duration: changed(
                old.duration(),
                duration.get_untracked().clamp(0, 3650) as u32,
            ),
        };
        sync.plan(plan, PlanUpdate::PlanBuffer(Crud::Update(patch)));
        apply_dependents(sync, plan, node, &was, &dependents.get_untracked());
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
            <Field label="Duration (days)">
                <Number value=duration on_input=Callback::new(move |n| duration.set(n)) min=0 max=3650 />
            </Field>
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
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <Dependents plan=plan me=node deps=dependents />
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
    let Some(group) = snapshot(plan, |p| p.task_groups().get(&id).cloned()) else {
        return view! { <p>"This group has been removed."</p> }.into_any();
    };
    let node = PlanNode::TaskGroup(id);
    let name = RwSignal::new(group.name().to_string());
    let parent = RwSignal::new(group.parent());
    let deps = RwSignal::new(group.dependencies().clone());
    let original = StoredValue::new(group);
    let was = snapshot(plan, |p| Some(dependents_of(p, node))).unwrap_or_default();
    let dependents = RwSignal::new(was.clone());
    let save = move |_| {
        let old = original.get_value();
        let patch = TaskGroupPatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            dependencies: changed(old.dependencies(), deps.get_untracked()),
            parent: changed(&old.parent(), parent.get_untracked()),
        };
        sync.plan(plan, PlanUpdate::TaskGroup(Crud::Update(patch)));
        apply_dependents(sync, plan, node, &was, &dependents.get_untracked());
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <Dependents plan=plan me=node deps=dependents />
            <Actions on_cancel=on_cancel on_delete=remove />
        </form>
    }
    .into_any()
}
// }}}
