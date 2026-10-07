//! One plan: its Gantt chart or list, and the editor for the item that is open.

use std::collections::HashSet;

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    planner::plan_update::PlanUpdate,
    protocol::Created,
    types::{
        crud::Crud,
        dependency::{Dependency, PlanNode},
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        short_id::ShortId,
        task::{NewTask, TaskPatch},
        task_group::NewTaskGroup,
    },
};
use uuid::Uuid;

use crate::{
    editor::ItemEditor,
    format,
    gantt::Gantt,
    model::{self, Look, Row},
    nav::{PlanTab, Route, navigate},
    settings::PlanSettings,
    ui::use_sync,
};

fn task_patch(id: Uuid) -> TaskPatch {
    TaskPatch {
        id,
        name: None,
        parent: None,
        mode: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        description: None,
        persistent_state: None,
    }
}

/// Anchors a new item to the start of its group, so it gets a place on the schedule
type Anchor = fn(Uuid, ShortId, &crate::sync::Sync, Box<dyn FnOnce(Option<Created>)>);

#[derive(Clone, Copy)]
enum Kind {
    Task,
    Milestone,
    Group,
    Buffer,
}

#[component]
pub fn PlanPage(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let route = expect_context::<RwSignal<Route>>();
    let tab = Memo::new(move |_| match route.get() {
        Route::Plan(_, tab, _) => tab,
        _ => PlanTab::Gantt,
    });
    let open = Memo::new(move |_| match route.get() {
        Route::Plan(_, _, open) => open,
        _ => None,
    });
    let exists = Memo::new(move |_| {
        sync.tenant
            .with(|t| t.as_ref().is_some_and(|t| t.plans().contains_key(&plan)))
    });
    let collapsed = RwSignal::new(HashSet::<Uuid>::new());
    let rows = Memo::new(move |_| {
        let schedule = sync.schedule.get();
        let state = schedule.as_ref().and_then(|(_, s)| s.plans.get(&plan));
        collapsed.with(|collapsed| {
            sync.tenant.with(|t| {
                t.as_ref()
                    .and_then(|t| t.plans().get(&plan))
                    .map(|p| model::rows(p, state, collapsed))
                    .unwrap_or_default()
            })
        })
    });
    let name = Memo::new(move |_| {
        sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.plans().get(&plan))
                .map(|p| p.name().to_string())
                .unwrap_or_default()
        })
    });
    let node = Memo::new(move |_| {
        let id = open.get()?;
        sync.tenant.with(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            model::nodes(p).into_iter().find(|n| n.id() == id)
        })
    });
    let today = Signal::derive(move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default())
    });
    let range = Signal::derive(move || {
        let spans: Vec<(NaiveDate, NaiveDate)> =
            rows.with(|r| r.iter().filter_map(|r| r.span).collect());
        let first = spans.iter().map(|s| s.0).min()?.min(today.get());
        let last = spans.iter().map(|s| s.1).max()?.max(today.get());
        Some((first, last))
    });
    let problems = Memo::new(move |_| {
        rows.with(|r| {
            (
                r.iter().filter(|r| r.unscheduled.is_some()).count(),
                r.iter().filter(|r| r.violated).count(),
            )
        })
    });
    let finish = Memo::new(move |_| {
        sync.schedule.with(|s| {
            s.as_ref()
                .and_then(|(_, s)| s.plans.get(&plan))
                .and_then(|s| s.plan_end)
        })
    });

    let go = move |tab: PlanTab, open: Option<Uuid>| navigate(Route::Plan(plan, tab, open));
    let on_select = Callback::new(move |node: PlanNode| go(tab.get_untracked(), Some(node.id())));
    let on_close = Callback::new(move |_| go(tab.get_untracked(), None));
    let on_toggle = Callback::new(move |node: PlanNode| {
        collapsed.update(|c| {
            if !c.remove(&node.id()) {
                c.insert(node.id());
            }
        });
    });

    // new items go in the group that is open, or beside the item that is
    let add = move |kind: Kind| {
        let parent = node
            .get_untracked()
            .and_then(|n| match n {
                PlanNode::TaskGroup(id) => Some(Parent::Node(id)),
                other => sync.tenant.with_untracked(|t| {
                    let p = t.as_ref()?.plans().get(&plan)?;
                    Some(match other {
                        PlanNode::Task(id) => p.tasks().get(&id)?.parent(),
                        PlanNode::Milestone(id) => p.milestones().get(&id)?.parent(),
                        PlanNode::PlanBuffer(id) => p.plan_buffers().get(&id)?.parent(),
                        PlanNode::TaskGroup(_) => unreachable!("a group is handled above"),
                    })
                }),
            })
            .unwrap_or(Parent::PlanStart);
        let nil = Uuid::nil();
        let (update, depends): (PlanUpdate, Anchor) = match kind {
            Kind::Task => (
                PlanUpdate::Task(Crud::Add(NewTask {
                    id: nil,
                    name: "New task".to_string(),
                    parent,
                })),
                |id, plan, sync, then| {
                    let patch = TaskPatch {
                        dependencies: Some(vec![Dependency::default()]),
                        ..task_patch(id)
                    };
                    sync.plan_then(plan, PlanUpdate::Task(Crud::Update(patch)), then);
                },
            ),
            Kind::Milestone => (
                PlanUpdate::Milestone(Crud::Add(NewMilestone {
                    id: nil,
                    name: "New milestone".to_string(),
                    parent,
                })),
                |id, plan, sync, then| {
                    let patch = plinko::types::milestone::MilestonePatch {
                        id,
                        name: None,
                        parent: None,
                        time_constraint: None,
                        dependencies: Some(vec![Dependency::default()]),
                        persistent_state: None,
                    };
                    sync.plan_then(plan, PlanUpdate::Milestone(Crud::Update(patch)), then);
                },
            ),
            Kind::Group => (
                PlanUpdate::TaskGroup(Crud::Add(NewTaskGroup {
                    id: nil,
                    name: "New group".to_string(),
                    parent,
                })),
                |id, plan, sync, then| {
                    let patch = plinko::types::task_group::TaskGroupPatch {
                        id,
                        name: None,
                        dependencies: Some(vec![Dependency::default()]),
                        parent: None,
                        placeholder_duration: None,
                    };
                    sync.plan_then(plan, PlanUpdate::TaskGroup(Crud::Update(patch)), then);
                },
            ),
            Kind::Buffer => (
                PlanUpdate::PlanBuffer(Crud::Add(NewPlanBuffer {
                    id: nil,
                    parent,
                    duration: 3,
                })),
                |id, plan, sync, then| {
                    let patch = plinko::types::plan_buffer::PlanBufferPatch {
                        id,
                        parent: None,
                        dependencies: Some(vec![Dependency::default()]),
                        set_start_day: None,
                        duration: None,
                    };
                    sync.plan_then(plan, PlanUpdate::PlanBuffer(Crud::Update(patch)), then);
                },
            ),
        };
        let current = tab.get_untracked();
        sync.plan_then(plan, update, move |created| {
            if let Some(Created::Item(id)) = created {
                // anchored to the start so it is scheduled, then open for editing
                let open = Box::new(move |_| navigate(Route::Plan(plan, current, Some(id))));
                depends(id, plan, &sync, open);
            }
        });
    };

    let tab_link = move |label: &'static str, to: PlanTab| {
        view! {
            <a href=Route::Plan(plan, to, None).path() class:active=move || tab.get() == to>{label}</a>
        }
    };
    view! {
        <Show when=move || exists.get() fallback=move || view! {
            <p class="empty">"This plan is not here, it may have been deleted. "<a href=Route::Plans.path()>"Back to plans"</a></p>
        }>
            <div class="plan">
                <div class="bar">
                    <h1>{move || name.get()}</h1>
                    {move || finish.get().map(|f| view! {
                        <span class="pill" class:late={f.delay() > 0}>
                            {format!("Finishes {} · {}", format::day(f.forecast), format::delay(f.delay()))}
                        </span>
                    })}
                    <Show when=move || { let n = problems.get().0; n > 0 }>
                        <span class="pill problem">{move || format!("{} not scheduled", problems.get().0)}</span>
                    </Show>
                    <Show when=move || { let n = problems.get().1; n > 0 }>
                        <span class="pill problem">{move || format!("{} miss their date", problems.get().1)}</span>
                    </Show>
                </div>
                <nav class="subtabs" aria-label="Plan views">
                    {tab_link("Gantt", PlanTab::Gantt)}
                    {tab_link("List", PlanTab::List)}
                    {tab_link("Settings", PlanTab::Settings)}
                </nav>
                <Show when=move || tab.get() != PlanTab::Settings>
                    <div class="tools">
                        <button on:click=move |_| add(Kind::Task)>"+ Task"</button>
                        <button on:click=move |_| add(Kind::Milestone)>"+ Milestone"</button>
                        <button on:click=move |_| add(Kind::Group)>"+ Group"</button>
                        <button on:click=move |_| add(Kind::Buffer)>"+ Buffer"</button>
                    </div>
                </Show>
                <div class="split">
                    <div class="main">
                        {move || match tab.get() {
                            PlanTab::Gantt => view! {
                                <Gantt
                                    rows=rows.into()
                                    today=today
                                    range=range
                                    selected=Signal::derive(move || node.get())
                                    on_select=on_select
                                    on_toggle=on_toggle
                                />
                            }.into_any(),
                            PlanTab::List => view! { <List rows=rows selected=node on_select=on_select on_toggle=on_toggle /> }.into_any(),
                            PlanTab::Settings => view! { <PlanSettings plan=plan /> }.into_any(),
                        }}
                    </div>
                    {move || {
                        node.get()
                            .filter(|_| tab.get() != PlanTab::Settings)
                            .map(|node| view! { <ItemEditor plan=plan node=node on_close=on_close /> })
                    }}
                </div>
            </div>
        </Show>
    }
}

#[component]
fn List(
    rows: Memo<Vec<Row>>,
    selected: Memo<Option<PlanNode>>,
    on_select: Callback<PlanNode>,
    on_toggle: Callback<PlanNode>,
) -> impl IntoView {
    view! {
        <ul class="items">
            <For each=move || rows.get() key=|r| format!("{r:?}") children=move |row| {
                let node = row.node;
                let look = match row.look {
                    Look::NotStarted => "not-started",
                    Look::InProgress => "in-progress",
                    Look::OnHold => "on-hold",
                    Look::Completed => "completed",
                };
                let dates = row.span.map(|(from, to)| {
                    let last = to.pred_opt().unwrap_or(from);
                    if last == from {
                        format::day(from)
                    } else {
                        format!("{} → {}", format::day(from), format::day(last))
                    }
                });
                view! {
                    <li style=format!("margin-left: {}px", row.depth * 16)>
                        {row.has_members.then(|| view! {
                            <button class="plain" aria-label="Show or hide what is in the group" on:click=move |_| on_toggle.run(node)>
                                {if row.collapsed { "▸" } else { "▾" }}
                            </button>
                        })}
                        <button
                            class=format!("item {look}")
                            class:selected=move || selected.get() == Some(node)
                            on:click=move |_| on_select.run(node)
                        >
                            <strong>{row.name.clone()}</strong>
                            <span class="dates">{dates.unwrap_or_else(|| "Not scheduled".to_string())}</span>
                            {row.unscheduled.map(|r| view! { <span class="problem">{format!("⚠ {}", model::reason(r))}</span> })}
                            {row.violated.then(|| view! { <span class="problem">"⚠ Misses its date"</span> })}
                        </button>
                    </li>
                }
            } />
        </ul>
    }
}
