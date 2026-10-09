//! One plan: its toolbar of icons, its Gantt chart or list, and the window for the item that is open.

use std::collections::HashSet;

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
    editor::{ItemEditor, remove_item},
    format,
    gantt::{self, Gantt, arrange::Mark, layout::Viewport, scene},
    icons::{Icon, IconKind},
    model::{self, Look, Row},
    nav::{PlanTab, Route, navigate},
    settings::PlanSettings,
    ui::{Modal, use_sync},
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
    let state_of = move || {
        sync.schedule
            .with(|s| s.as_ref().and_then(|(_, s)| s.plans.get(&plan).cloned()))
    };
    let rows = Memo::new(move |_| {
        let state = state_of();
        collapsed.with(|collapsed| {
            sync.tenant.with(|t| {
                t.as_ref()
                    .and_then(|t| t.plans().get(&plan))
                    .map(|p| model::rows(p, state.as_ref(), collapsed))
                    .unwrap_or_default()
            })
        })
    });
    let scene = Memo::new(move |_| {
        let state = state_of();
        collapsed
            .with(|collapsed| {
                sync.tenant.with(|t| {
                    let plan = t.as_ref().and_then(|t| t.plans().get(&plan))?;
                    Some(scene::build(plan, state.as_ref(), collapsed))
                })
            })
            .unwrap_or_else(|| scene::Scene {
                plan_start: sync
                    .tenant
                    .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default()),
                arrangement: Default::default(),
                unplaced: Vec::new(),
                last_day: 0.0,
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
    let unplaced = Memo::new(move |_| scene.with(|s| s.unplaced.len()));

    let viewport = RwSignal::new(None::<Viewport>);
    let jump = RwSignal::new(0u32);
    let focus = RwSignal::new(None::<Mark>);
    let searching = RwSignal::new(false);
    let listing = RwSignal::new(false);

    let links = Callback::new(move |mark: Mark| {
        sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.plans().get(&plan))
                .map(|p| gantt::links::links(p, state_of().as_ref(), mark))
                .unwrap_or_default()
        })
    });
    let info = Callback::new(move |mark: Mark| {
        let state = state_of();
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                Some(gantt::info::info(
                    t,
                    t.plans().get(&plan)?,
                    state.as_ref(),
                    mark,
                ))
            })
            .unwrap_or_default()
    });
    let violation =
        Callback::new(move |mark: Mark| gantt::info::violation(state_of().as_ref(), mark));

    // an item just added and not saved yet, which is dropped again if the window is cancelled
    let fresh = RwSignal::new(None::<Uuid>);
    let go = move |tab: PlanTab, open: Option<Uuid>| navigate(Route::Plan(plan, tab, open));
    let on_open = Callback::new(move |node: PlanNode| go(tab.get_untracked(), Some(node.id())));
    let on_close = Callback::new(move |_| go(tab.get_untracked(), None));
    let on_toggle = Callback::new(move |node: PlanNode| {
        collapsed.update(|c| {
            if !c.remove(&node.id()) {
                c.insert(node.id());
            }
        });
    });
    // what a search picked: on the chart scroll to it and flash it, else open it
    let on_found = Callback::new(move |node: PlanNode| {
        searching.set(false);
        let placed = scene.with_untracked(|s| {
            s.arrangement
                .placed
                .iter()
                .any(|p| p.mark == Mark::Node(node))
        });
        if tab.get_untracked() == PlanTab::Gantt && placed {
            focus.set(Some(Mark::Node(node)));
        } else if tab.get_untracked() == PlanTab::Gantt {
            // inside a folded group: unfold what holds it
            let mut inside = model::parent_of_in(sync, plan, node);
            while let Some(group) = inside {
                collapsed.update(|c| {
                    c.remove(&group);
                });
                inside = model::parent_of_in(sync, plan, PlanNode::TaskGroup(group));
            }
            set_timeout(
                move || focus.set(Some(Mark::Node(node))),
                std::time::Duration::from_millis(50),
            );
        } else {
            go(tab.get_untracked(), Some(node.id()));
        }
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
                let open = Box::new(move |_| {
                    fresh.try_set(Some(id));
                    navigate(Route::Plan(plan, current, Some(id)));
                });
                depends(id, plan, &sync, open);
            }
        });
    };

    let icon_button = move |label: &'static str, kind: IconKind, on: Callback<()>| {
        view! {
            <button type="button" class="toolbar-btn" title=label aria-label=label on:click=move |_| on.run(())>
                <Icon kind=kind />
            </button>
        }
    };
    view! {
        <Show when=move || exists.get() fallback=move || view! {
            <p class="empty">"This plan is not here, it may have been deleted. "<a href=Route::Plans.path()>"Back to plans"</a></p>
        }>
            <div class="plan">
                <div class="toolbar">
                    <div class="toolbar-buttons">
                    <div class="toolbar-left">
                        <Show
                            when=move || tab.get() != PlanTab::Settings
                            fallback=move || icon_button("Back", IconKind::Back, Callback::new(move |_| go(PlanTab::Gantt, None)))
                        >
                            {icon_button("Back to plans", IconKind::Back, Callback::new(move |_| navigate(Route::Plans)))}
                            <Show when=move || tab.get() == PlanTab::Gantt>
                                {icon_button("Jump to today", IconKind::Today, Callback::new(move |_| jump.update(|n| *n += 1)))}
                            </Show>
                            {icon_button("Add task", IconKind::AddTask, Callback::new(move |_| add(Kind::Task)))}
                            {icon_button("Add milestone", IconKind::AddMilestone, Callback::new(move |_| add(Kind::Milestone)))}
                            {icon_button("Add task group", IconKind::AddGroup, Callback::new(move |_| add(Kind::Group)))}
                            {icon_button("Add buffer", IconKind::AddBuffer, Callback::new(move |_| add(Kind::Buffer)))}
                            {icon_button("Search", IconKind::Search, Callback::new(move |_| searching.set(true)))}
                        </Show>
                    </div>
                    <span class="toolbar-title">
                        <span class="toolbar-title-text">{move || name.get()}</span>
                    </span>
                    <div class="toolbar-right">
                        <Show when=move || { unplaced.get() > 0 }>
                            <button
                                type="button"
                                class="toolbar-btn problem"
                                title="Not scheduled"
                                aria-label="Items that are not scheduled"
                                on:click=move |_| listing.set(true)
                            >
                                <Icon kind=IconKind::Alert />
                                <span class="badge">{move || unplaced.get()}</span>
                            </button>
                        </Show>
                        {move || {
                            let (kind, label, to) = if tab.get() == PlanTab::List {
                                (IconKind::Gantt, "Show the Gantt chart", PlanTab::Gantt)
                            } else {
                                (IconKind::List, "Show a list", PlanTab::List)
                            };
                            icon_button(label, kind, Callback::new(move |_| go(to, None)))
                        }}
                        <button
                            type="button"
                            class="toolbar-btn"
                            class:active=move || tab.get() == PlanTab::Settings
                            title="Plan settings"
                            aria-label="Plan settings"
                            on:click=move |_| go(if tab.get_untracked() == PlanTab::Settings { PlanTab::Gantt } else { PlanTab::Settings }, None)
                        >
                            <Icon kind=IconKind::Settings />
                        </button>
                    </div>
                    </div>
                </div>
                <div class="workspace">
                <div class="main">
                    {move || match tab.get() {
                        PlanTab::Gantt => view! {
                            <Gantt
                                scene=scene.into()
                                today=today
                                viewport=viewport
                                links=links
                                info=info
                                violation=violation
                                on_open=on_open
                                on_toggle=on_toggle
                                jump=jump
                                focus=focus
                            />
                        }.into_any(),
                        PlanTab::List => view! { <List rows=rows on_open=on_open on_toggle=on_toggle /> }.into_any(),
                        PlanTab::Settings => view! { <PlanSettings plan=plan /> }.into_any(),
                    }}
                </div>
                {move || {
                    node.get()
                        .filter(|_| tab.get() != PlanTab::Settings)
                        .map(|node| view! { <ItemPane plan=plan node=node fresh=fresh on_close=on_close /> })
                }}
                </div>
                <Show when=move || searching.get()>
                    <SearchWindow plan=plan on_found=on_found on_close=Callback::new(move |_| searching.set(false)) />
                </Show>
                <Show when=move || listing.get()>
                    <UnplacedWindow
                        scene=scene
                        on_open=Callback::new(move |node: PlanNode| {
                            listing.set(false);
                            go(tab.get_untracked(), Some(node.id()));
                        })
                        on_close=Callback::new(move |_| listing.set(false))
                    />
                </Show>
            </div>
        </Show>
    }
}

/// The pane beside the chart for editing the item that is open
#[component]
fn ItemPane(
    plan: ShortId,
    node: PlanNode,
    fresh: RwSignal<Option<Uuid>>,
    on_close: Callback<()>,
) -> impl IntoView {
    let title = match node {
        PlanNode::Task(_) => "Edit Task",
        PlanNode::Milestone(_) => "Edit Milestone",
        PlanNode::PlanBuffer(_) => "Edit Buffer",
        PlanNode::TaskGroup(_) => "Edit Task Group",
    };
    let saved = Callback::new(move |()| {
        fresh.set(None);
        on_close.run(());
    });
    let cancel = Callback::new(move |()| {
        if fresh.get_untracked() == Some(node.id()) {
            fresh.set(None);
            remove_item(plan, node);
        }
        on_close.run(());
    });
    view! {
        <aside class="pane">
            <header>
                <h2>{title}</h2>
                <button type="button" class="plain" aria-label="Close" on:click=move |_| cancel.run(())>"✕"</button>
            </header>
            <div class="body">
                <ItemEditor plan=plan node=node on_close=saved on_cancel=cancel />
            </div>
        </aside>
    }
}

/// Filter the items by name, then go to one
#[component]
fn SearchWindow(
    plan: ShortId,
    on_found: Callback<PlanNode>,
    on_close: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let filter = RwSignal::new(String::new());
    let results = Memo::new(move |_| {
        let wanted = filter.get().to_lowercase();
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut found: Vec<(PlanNode, String)> = model::nodes(p)
                    .into_iter()
                    .map(|n| (n, model::node_name(p, n)))
                    .filter(|(_, name)| wanted.is_empty() || name.to_lowercase().contains(&wanted))
                    .collect();
                found.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.id().cmp(&b.0.id())));
                Some(found)
            })
            .unwrap_or_default()
    });
    view! {
        <Modal title="Search" on_close=on_close>
            <input
                type="text"
                class="search"
                placeholder="Filter tasks and milestones…"
                autofocus
                prop:value=move || filter.get()
                class:nothing=move || !filter.with(String::is_empty) && results.with(Vec::is_empty)
                on:input=move |ev| filter.set(event_target_value(&ev))
            />
            <ul class="results">
                <Show when=move || results.with(Vec::is_empty)>
                    <li class="none">"No results"</li>
                </Show>
                <For each=move || results.get() key=|r| r.clone() children=move |(node, name)| {
                    let glyph = match node {
                        PlanNode::Task(_) => "▬",
                        PlanNode::Milestone(_) => "◇",
                        PlanNode::PlanBuffer(_) => "▭",
                        PlanNode::TaskGroup(_) => "▢",
                    };
                    view! {
                        <li>
                            <button type="button" on:click=move |_| on_found.run(node)>
                                <span class="glyph">{glyph}</span>
                                {name}
                            </button>
                        </li>
                    }
                } />
            </ul>
        </Modal>
    }
}

/// Everything the scheduler could not place, and why
#[component]
fn UnplacedWindow(
    scene: Memo<scene::Scene>,
    on_open: Callback<PlanNode>,
    on_close: Callback<()>,
) -> impl IntoView {
    view! {
        <Modal title="Not scheduled" on_close=on_close>
            <p class="small">"These have no place on the chart. Open one to fix what is stopping it."</p>
            <ul class="results">
                <For each=move || scene.with(|s| s.unplaced.clone()) key=|u| format!("{u:?}") children=move |u| {
                    let node = u.node;
                    view! {
                        <li>
                            <button type="button" on:click=move |_| on_open.run(node)>
                                <strong>{u.name.clone()}</strong>
                                <span class="problem">{u.reason.clone()}</span>
                            </button>
                        </li>
                    }
                } />
            </ul>
        </Modal>
    }
}

#[component]
fn List(
    rows: Memo<Vec<Row>>,
    on_open: Callback<PlanNode>,
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
                            <button type="button" class="plain" aria-label="Show or hide what is in the group" on:click=move |_| on_toggle.run(node)>
                                {if row.collapsed { "▸" } else { "▾" }}
                            </button>
                        })}
                        <button
                            class=format!("item {look}")
                            on:click=move |_| on_open.run(node)
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
