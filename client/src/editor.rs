//! Editing one item of a plan. Each editor keeps a draft of its fields and sends only what was
//! changed, so two people editing different fields of an item do not undo each other.

use std::collections::HashSet;

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
    format, forms,
    model::{self, node_name},
    ui::{Field, Number, Select, Text, options, use_sync},
};

/// What a plan holds right now, or `None` if it is gone
fn read<R>(plan: ShortId, f: impl FnOnce(&Plan) -> Option<R>) -> Option<R> {
    let sync = use_sync();
    sync.tenant.with(|t| f(t.as_ref()?.plans().get(&plan)?))
}

fn changed<T: PartialEq>(old: &T, new: T) -> Option<T> {
    (*old != new).then_some(new)
}

#[component]
pub fn ItemEditor(plan: ShortId, node: PlanNode, on_close: Callback<()>) -> impl IntoView {
    let kind = match node {
        PlanNode::Task(_) => "Task",
        PlanNode::Milestone(_) => "Milestone",
        PlanNode::PlanBuffer(_) => "Buffer",
        PlanNode::TaskGroup(_) => "Task group",
    };
    let sync = use_sync();
    let remove = move |_| {
        let update = match node {
            PlanNode::Task(id) => PlanUpdate::Task(Crud::Remove(id)),
            PlanNode::Milestone(id) => PlanUpdate::Milestone(Crud::Remove(id)),
            PlanNode::PlanBuffer(id) => PlanUpdate::PlanBuffer(Crud::Remove(id)),
            PlanNode::TaskGroup(id) => PlanUpdate::TaskGroup(Crud::Remove(id)),
        };
        sync.plan(plan, update);
        on_close.run(());
    };
    view! {
        <aside class="editor" aria-label=format!("Edit {kind}")>
            <header>
                <h2>{kind}</h2>
                <button class="plain" aria-label="Close" on:click=move |_| on_close.run(())>"✕"</button>
            </header>
            <Info plan=plan node=node />
            {match node {
                PlanNode::Task(id) => view! { <TaskForm plan=plan id=id on_close=on_close /> }.into_any(),
                PlanNode::Milestone(id) => view! { <MilestoneForm plan=plan id=id on_close=on_close /> }.into_any(),
                PlanNode::PlanBuffer(id) => view! { <BufferForm plan=plan id=id on_close=on_close /> }.into_any(),
                PlanNode::TaskGroup(id) => view! { <GroupForm plan=plan id=id on_close=on_close /> }.into_any(),
            }}
            <button class="danger" on:click=remove>{format!("Delete {}", kind.to_lowercase())}</button>
        </aside>
    }
}

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

// pickers {{{
/// Every item of the plan that something can depend on, other than `except`
fn depends_options(plan: ShortId, except: PlanNode) -> Signal<Vec<(String, String)>> {
    Signal::derive(move || {
        let mut options = vec![(
            "start".to_string(),
            "Start of the group or plan".to_string(),
        )];
        let mut nodes = read(plan, |p| Some(model::nodes(p))).unwrap_or_default();
        nodes.retain(|n| *n != except);
        let mut named: Vec<(String, String)> = read(plan, |p| {
            Some(
                nodes
                    .iter()
                    .map(|n| {
                        (
                            forms::node_value(DependencyNode::Node(*n)),
                            node_name(p, *n),
                        )
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
        named.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        options.extend(named);
        options
    })
}

#[component]
fn Dependencies(plan: ShortId, me: PlanNode, deps: RwSignal<Vec<Dependency>>) -> impl IntoView {
    let choices = depends_options(plan, me);
    view! {
        <fieldset class="list">
            <legend>"Depends on"</legend>
            <For each=move || 0..deps.with(Vec::len) key=|i| *i children=move |i| {
                let node = Signal::derive(move || {
                    deps.with(|d| d.get(i).map(|d| forms::node_value(d.node)).unwrap_or_default())
                });
                let lag = Signal::derive(move || deps.with(|d| d.get(i).map_or(0, |d| i64::from(d.lag))));
                view! {
                    <div class="row">
                        <Select
                            value=node
                            options=choices
                            on_change=Callback::new(move |v: String| {
                                if let Some(node) = forms::parse_node(&v) {
                                    deps.update(|d| if let Some(d) = d.get_mut(i) { d.node = node });
                                }
                            })
                        />
                        <span class="small">"lag"</span>
                        <Number
                            value=lag
                            on_input=Callback::new(move |n: i64| {
                                let n = n.clamp(-3650, 3650) as i32;
                                deps.update(|d| if let Some(d) = d.get_mut(i) { d.lag = n });
                            })
                        />
                        <button class="plain" aria-label="Remove dependency" on:click=move |_| {
                            deps.update(|d| { if i < d.len() { d.remove(i); } });
                        }>"✕"</button>
                    </div>
                }
            } />
            <button class="plain" on:click=move |_| deps.update(|d| d.push(Dependency::default()))>
                "+ Add a dependency"
            </button>
        </fieldset>
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
        <Field label="In group">
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

#[component]
fn Constraint(kind: RwSignal<String>, day: RwSignal<NaiveDate>) -> impl IntoView {
    view! {
        <Field label="Date">
            <div class="row">
                <Select
                    value=kind
                    options=options([
                        ("none", "No date"),
                        ("earliest", "Not before"),
                        ("latest", "Not after"),
                        ("fixed", "On"),
                    ])
                    on_change=Callback::new(move |v| kind.set(v))
                />
                <Show when=move || kind.get() != "none">
                    <DateInput day=day />
                </Show>
            </div>
        </Field>
    }
}

#[component]
pub fn DateInput(day: RwSignal<NaiveDate>) -> impl IntoView {
    view! {
        <input
            type="date"
            prop:value=move || format::iso(day.get())
            on:input=move |ev| {
                if let Some(d) = format::parse_iso(&event_target_value(&ev)) {
                    day.set(d);
                }
            }
        />
    }
}

/// A date that can be left empty
#[component]
fn OptionalDate(value: RwSignal<Option<NaiveDate>>) -> impl IntoView {
    view! {
        <input
            type="date"
            prop:value=move || value.get().map(format::iso).unwrap_or_default()
            on:input=move |ev| value.set(format::parse_iso(&event_target_value(&ev)))
        />
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
    let choices = worker_options(plan);
    let hours = options([
        ("0", "0 h"),
        ("1", "1 h"),
        ("2", "2 h"),
        ("3", "3 h"),
        ("4", "4 h"),
        ("5", "5 h"),
        ("6", "6 h"),
        ("7", "7 h"),
        ("8", "8 h"),
    ]);
    view! {
        <fieldset class="list">
            <legend>"Who works on it, hours a day"</legend>
            <For each=move || 0..allocations.with(Vec::len) key=|i| *i children=move |i| {
                let worker = Signal::derive(move || {
                    allocations.with(|a| a.get(i).map(|(w, _)| forms::worker_value(*w)).unwrap_or_default())
                });
                let hour = Signal::derive(move || {
                    allocations.with(|a| a.get(i).map(|(_, h)| h.to_string()).unwrap_or_default())
                });
                view! {
                    <div class="row">
                        <Select
                            value=worker
                            options=choices
                            on_change=Callback::new(move |v: String| {
                                if let Some(w) = forms::parse_worker(&v) {
                                    allocations.update(|a| if let Some(a) = a.get_mut(i) { a.0 = w });
                                }
                            })
                        />
                        <Select
                            value=hour
                            options=hours
                            on_change=Callback::new(move |v: String| {
                                if let Ok(h) = v.parse::<u8>() {
                                    allocations.update(|a| if let Some(a) = a.get_mut(i) { a.1 = h });
                                }
                            })
                        />
                        <button class="plain" aria-label="Remove" on:click=move |_| {
                            allocations.update(|a| { if i < a.len() { a.remove(i); } });
                        }>"✕"</button>
                    </div>
                }
            } />
            <button class="plain" on:click=move |_| {
                allocations.update(|a| {
                    if !a.iter().any(|(w, _)| *w == Worker::Placeholder) {
                        a.push((Worker::Placeholder, 8));
                    }
                });
            }>"+ Add someone"</button>
        </fieldset>
    }
}
// }}}

// forms {{{
#[component]
fn TaskForm(plan: ShortId, id: Uuid, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let Some(task) = read(plan, |p| p.tasks().get(&id).cloned()) else {
        return view! { <p>"This task has been removed."</p> }.into_any();
    };
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let node = PlanNode::Task(id);
    let name = RwSignal::new(task.name().to_string());
    let description = RwSignal::new(task.description().unwrap_or_default().to_string());
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
        let description = description.get_untracked();
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
            actual_workers: changed(old.actual_workers(), recorded.get_untracked()),
            description: changed(
                &old.description().map(str::to_string),
                (!description.is_empty()).then_some(description),
            ),
            persistent_state: changed(&old.persistent_state(), new_state),
        };
        sync.plan(plan, PlanUpdate::Task(Crud::Update(patch)));
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Field label="Notes">
                <textarea prop:value=move || description.get() on:input=move |ev| description.set(event_target_value(&ev))></textarea>
            </Field>
            <Field label="Days of work">
                <Number value=duration on_input=Callback::new(move |n| duration.set(n)) min=1 max=3650 />
            </Field>
            <Field label="Status">
                <Select
                    value=Signal::derive(move || forms::status_value(status.get()).to_string())
                    options=options([
                        ("not-started", "Not started"),
                        ("in-progress", "In progress"),
                        ("on-hold", "On hold"),
                        ("completed", "Completed"),
                        ("dropped", "Dropped"),
                    ])
                    on_change=Callback::new(move |v: String| {
                        if let Some(s) = forms::parse_status(&v) {
                            status.set(s);
                        }
                    })
                />
            </Field>
            <Show when=move || status.get() != Status::NotStarted>
                <Field label="Started"><OptionalDate value=started /></Field>
            </Show>
            <Show when=move || matches!(status.get(), Status::Completed | Status::OnHold)>
                <Field label="Finished"><OptionalDate value=ended /></Field>
            </Show>
            <Field label="Days worked">
                <Select
                    value=mode
                    options=options([
                        ("strict", "Whole days in a row"),
                        ("relaxed", "Any free time"),
                    ])
                    on_change=Callback::new(move |v| mode.set(v))
                />
            </Field>
            <Constraint kind=kind day=day />
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <Workers plan=plan allocations=allocations />
            <Recorded plan=plan task=original recorded=recorded allocations=allocations />
            <Affinities plan=plan chosen=affinities />
            <button type="submit" class="primary">"Save"</button>
        </form>
    }
    .into_any()
}

/// Who really did the work, for the people asked for
#[component]
fn Recorded(
    plan: ShortId,
    task: StoredValue<Task>,
    recorded: RwSignal<std::collections::HashMap<Worker, Assignee>>,
    allocations: RwSignal<Vec<(Worker, u8)>>,
) -> impl IntoView {
    let _ = task;
    let choices = Signal::derive(move || {
        let mut options = vec![("auto".to_string(), "Whoever is scheduled".to_string())];
        options.extend(worker_options(plan).get().into_iter().filter_map(|(v, l)| {
            let user = forms::parse_worker(&v).and_then(|w| match w {
                Worker::User(id) => Some(Assignee::User(id)),
                Worker::Placeholder => Some(Assignee::Placeholder),
                _ => None,
            })?;
            Some((forms::assignee_value(Some(user)), l))
        }));
        options
    });
    view! {
        <Show when=move || recorded.with(|r| !r.is_empty())>
            <fieldset class="list">
                <legend>"Who actually did the work"</legend>
                {move || {
                    let asked: Vec<Worker> = allocations.with(|a| a.iter().map(|(w, _)| *w).collect());
                    asked
                        .into_iter()
                        .map(|worker| {
                            let current = Signal::derive(move || {
                                forms::assignee_value(recorded.with(|r| r.get(&worker).copied()))
                            });
                            view! {
                                <div class="row">
                                    <span>{worker_label(plan, worker)}</span>
                                    <Select
                                        value=current
                                        options=choices
                                        on_change=Callback::new(move |v: String| {
                                            if let Some(choice) = forms::parse_assignee(&v) {
                                                recorded.update(|r| match choice {
                                                    Some(who) => { r.insert(worker, who); }
                                                    None => { r.remove(&worker); }
                                                });
                                            }
                                        })
                                    />
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </fieldset>
        </Show>
    }
}

fn worker_label(plan: ShortId, worker: Worker) -> String {
    worker_options(plan)
        .get_untracked()
        .into_iter()
        .find(|(v, _)| *v == forms::worker_value(worker))
        .map_or("Someone".to_string(), |(_, l)| l)
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
            <fieldset class="list">
                <legend>"Only on the days of"</legend>
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
            </fieldset>
        </Show>
    }
}

#[component]
fn MilestoneForm(plan: ShortId, id: Uuid, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let Some(milestone) = read(plan, |p| p.milestones().get(&id).cloned()) else {
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
            <button type="submit" class="primary">"Save"</button>
        </form>
    }
    .into_any()
}

#[component]
fn BufferForm(plan: ShortId, id: Uuid, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let Some(buffer) = read(plan, |p| p.plan_buffers().get(&id).cloned()) else {
        return view! { <p>"This buffer has been removed."</p> }.into_any();
    };
    let node = PlanNode::PlanBuffer(id);
    let duration = RwSignal::new(i64::from(*buffer.duration()));
    let set = RwSignal::new(*buffer.set_start_day());
    let parent = RwSignal::new(buffer.parent());
    let deps = RwSignal::new(buffer.dependencies().clone());
    let original = StoredValue::new(buffer);
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
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Working days">
                <Number value=duration on_input=Callback::new(move |n| duration.set(n)) min=0 max=3650 />
            </Field>
            <Field label="Meant to start on (it shrinks or grows to keep its end)">
                <OptionalDate value=set />
            </Field>
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <button type="submit" class="primary">"Save"</button>
        </form>
    }
    .into_any()
}

#[component]
fn GroupForm(plan: ShortId, id: Uuid, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let Some(group) = read(plan, |p| p.task_groups().get(&id).cloned()) else {
        return view! { <p>"This group has been removed."</p> }.into_any();
    };
    let node = PlanNode::TaskGroup(id);
    let name = RwSignal::new(group.name().to_string());
    let placeholder = RwSignal::new(i64::from(group.placeholder_duration()));
    let parent = RwSignal::new(group.parent());
    let deps = RwSignal::new(group.dependencies().clone());
    let original = StoredValue::new(group);
    let save = move |_| {
        let old = original.get_value();
        let patch = TaskGroupPatch {
            id,
            name: changed(&old.name().to_string(), name.get_untracked()),
            dependencies: changed(old.dependencies(), deps.get_untracked()),
            parent: changed(&old.parent(), parent.get_untracked()),
            placeholder_duration: changed(
                &old.placeholder_duration(),
                placeholder.get_untracked().clamp(1, 3650) as u32,
            ),
        };
        sync.plan(plan, PlanUpdate::TaskGroup(Crud::Update(patch)));
        on_close.run(());
    };
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); save(()); }>
            <Field label="Name">
                <Text value=name on_input=Callback::new(move |v| name.set(v)) />
            </Field>
            <Field label="Days when it holds nothing yet">
                <Number value=placeholder on_input=Callback::new(move |n| placeholder.set(n)) min=1 max=3650 />
            </Field>
            <ParentPicker plan=plan me=node parent=parent />
            <Dependencies plan=plan me=node deps=deps />
            <button type="submit" class="primary">"Save"</button>
        </form>
    }
    .into_any()
}
// }}}
