//! One plan: its toolbar of icons, its Gantt chart or list, and the window for the item that is open.

// modules {{{
use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use plinko::{
    availability::workload::Workload,
    common::{crud::Crud, short_id::ShortId},
    planner::items::{
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        task::{NewTask, TaskPatch, Worker},
        task_group::NewTaskGroup,
    },
    planner::plan_update::PlanUpdate,
    protocol::Created,
    scheduler::state::PlanVolatileState,
};
use uuid::Uuid;

use crate::{
    chart_edit,
    context::ContextBar,
    data::DataWindow,
    editor::{ItemEditor, remove_item},
    format,
    gantt::{self, Gantt, arrange::Mark, layout::Viewport, scene},
    guard::{ConfirmLeave, Guard},
    icons::{Icon, IconKind},
    model::{self, Look, Row},
    nav::{PlanTab, Route, navigate},
    settings::PlanSettings,
    ui::{Modal, use_sync},
};
// }}}

// task_patch {{{
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
        persistent_state: None,
    }
}
// }}}

// types {{{
/// Sets up a new item once it exists: what it depends on to begin with, and for a task who does it
type Anchor =
    fn(Uuid, ShortId, &crate::sync::Sync, Option<Dependency>, Box<dyn FnOnce(Option<Created>)>);

#[derive(Clone, Copy)]
enum Kind {
    Task,
    Milestone,
    Group,
    Buffer,
}
// }}}

// PlanPage {{{
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
    // what the scheduler made of this plan, looked at where it is rather than copied out
    let with_state = move |f: &mut dyn FnMut(Option<&PlanVolatileState>)| {
        sync.schedule
            .with(|s| f(s.as_ref().and_then(|(_, s)| s.plans.get(&plan))))
    };
    let rows = Memo::new(move |_| {
        let mut rows = Vec::new();
        with_state(&mut |state| {
            rows = collapsed.with(|collapsed| {
                sync.tenant.with(|t| {
                    t.as_ref()
                        .and_then(|t| t.plans().get(&plan))
                        .map(|p| model::rows(p, state, collapsed))
                        .unwrap_or_default()
                })
            });
        });
        rows
    });
    let scene = Memo::new(move |_| {
        let mut built = None;
        with_state(&mut |state| {
            built = collapsed.with(|collapsed| {
                sync.tenant.with(|t| {
                    let plan = t.as_ref().and_then(|t| t.plans().get(&plan))?;
                    Some(scene::build(plan, state, collapsed, t.as_ref()?.today()))
                })
            });
        });
        built.unwrap_or_else(|| {
            scene::Scene::empty(
                sync.tenant
                    .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default()),
            )
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
    // a task asked for from another page is put in the middle of the chart
    let reveal = RwSignal::new(None::<Mark>);
    let asked = expect_context::<crate::nav::Reveal>().0;
    if let Some((_, task)) = asked.get_untracked().filter(|(of, _)| *of == plan) {
        reveal.set(Some(PlanNode::Task(task)));
        asked.set(None);
    }
    let searching = RwSignal::new(false);
    let listing = RwSignal::new(false);

    let links = Callback::new(move |mark: Mark| {
        let mut found = Vec::new();
        with_state(&mut |state| {
            found = sync.tenant.with(|t| {
                t.as_ref()
                    .and_then(|t| t.plans().get(&plan))
                    .map(|p| gantt::links::links(p, state, mark))
                    .unwrap_or_default()
            });
        });
        found
    });
    let info = Callback::new(move |mark: Mark| {
        let mut said = Vec::new();
        with_state(&mut |state| {
            said = sync
                .tenant
                .with(|t| {
                    let t = t.as_ref()?;
                    Some(gantt::info::info(t, t.plans().get(&plan)?, state, mark))
                })
                .unwrap_or_default();
        });
        said
    });
    let violation = Callback::new(move |mark: Mark| {
        let mut said = Vec::new();
        with_state(&mut |state| said = gantt::info::violation(state, mark));
        said
    });

    let selected = RwSignal::new(None::<PlanNode>);
    let any_selected = Memo::new(move |_| selected.get().is_some());
    crate::escape::on_escape(
        move || any_selected.get(),
        Callback::new(move |()| selected.set(None)),
    );
    let open_node_exists = Memo::new(move |_| node.get().is_some());
    let data_of = RwSignal::new(None::<PlanNode>);
    // the selected item, until it is taken away
    let chosen = Memo::new(move |_| {
        let node = selected.get()?;
        scene
            .with(|s| s.arrangement.placed.iter().any(|p| p.mark == node))
            .then_some(node)
    });
    // where the bar under it goes on the chart
    let bar_at = Memo::new(move |_| {
        let node = chosen.get()?;
        let vp = viewport.get().unwrap_or_default();
        scene.with(|s| {
            let placed = s.arrangement.placed.iter().find(|p| p.mark == node)?;
            let day = (placed.span.0 - s.origin).num_days() as f64;
            let left = vp.x(day).max(4.0);
            let top = vp.lane_top(placed.lane) + gantt::layout::ROW_HEIGHT;
            Some((left, top))
        })
    });
    // an item just added and not saved yet, which is dropped again if the window is cancelled
    let fresh = RwSignal::new(None::<Uuid>);
    let go = move |tab: PlanTab, open: Option<Uuid>| navigate(Route::Plan(plan, tab, open));
    let guard = Guard::new();
    // an item is opened or the pane closed only after asking about changes not yet saved
    let on_open = Callback::new(move |node: PlanNode| {
        guard.then(Callback::new(move |()| {
            go(tab.get_untracked(), Some(node.id()))
        }));
    });
    // a tap on the chart selects, and with an edit pane open that moves to the item tapped
    let on_tap = Callback::new(move |node: PlanNode| {
        selected.set(Some(node));
        if tab.get_untracked() == PlanTab::Gantt && open_node_exists.get_untracked() {
            on_open.run(node);
        }
    });
    let on_close = Callback::new(move |_| go(tab.get_untracked(), None));
    // leaving a new item that was never saved takes it away again, as Cancel does
    let leave = Callback::new(move |()| {
        let item = node.get_untracked();
        if let Some(item) = item.filter(|i| fresh.get_untracked() == Some(i.id())) {
            fresh.set(None);
            remove_item(sync, plan, item);
        }
        on_close.run(());
    });
    let on_blank = Callback::new(move |()| {
        selected.set(None);
        if node.get_untracked().is_some() {
            guard.then(leave);
        }
    });
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
        let placed = scene.with_untracked(|s| s.arrangement.placed.iter().any(|p| p.mark == node));
        if tab.get_untracked() == PlanTab::Gantt && placed {
            focus.set(Some(node));
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
                move || focus.set(Some(node)),
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
                        PlanNode::PlanDelay(id) => p.plan_delays().get(&id)?.parent(),
                        PlanNode::TaskGroup(_) => unreachable!("a group is handled above"),
                    })
                }),
            })
            .unwrap_or(Parent::Plan);
        let nil = Uuid::nil();
        let (update, depends): (PlanUpdate, Anchor) = match kind {
            Kind::Task => (
                PlanUpdate::Task(Crud::Add(NewTask {
                    id: nil,
                    name: "New task".to_string(),
                    parent,
                })),
                |id, plan, sync, start, then| {
                    // the person creating it does it for half a day by default
                    let me = sync.me.get_untracked().map(|m| Worker::User(m.id));
                    let patch = TaskPatch {
                        dependencies: start.map(|d| vec![d]),
                        allocations: me.zip(Workload::new(4).ok()).map(|w| HashMap::from([w])),
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
                |id, plan, sync, start, then| {
                    let patch = plinko::planner::items::milestone::MilestonePatch {
                        id,
                        name: None,
                        parent: None,
                        time_constraint: None,
                        dependencies: start.map(|d| vec![d]),
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
                |id, plan, sync, start, then| {
                    let patch = plinko::planner::items::task_group::TaskGroupPatch {
                        id,
                        name: None,
                        dependencies: start.map(|d| vec![d]),
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
                |id, plan, sync, start, then| {
                    let patch = plinko::planner::items::plan_buffer::PlanBufferPatch {
                        id,
                        parent: None,
                        dependencies: start.map(|d| vec![d]),
                        set_start_day: None,
                        duration: None,
                    };
                    sync.plan_then(plan, PlanUpdate::PlanBuffer(Crud::Update(patch)), then);
                },
            ),
        };
        // top level items begin from the plan's default start node when it has one
        let start = sync.tenant.with_untracked(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            let node = p
                .scheduler_data()
                .default_start()
                .filter(|_| parent == Parent::Plan)?;
            Some(Dependency {
                node: DependencyNode::Node(node),
                lag: 0,
            })
        });
        let current = tab.get_untracked();
        sync.plan_then(plan, update, move |created| {
            if let Some(Created::Item(id)) = created {
                // anchored to the start so it is scheduled, then open for editing
                let open = Box::new(move |_| {
                    fresh.try_set(Some(id));
                    navigate(Route::Plan(plan, current, Some(id)));
                });
                depends(id, plan, &sync, start, open);
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
                            <AddMenu on_add=Callback::new(move |kind: Kind| add(kind)) />
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
                <div class="main" on:click=move |ev| if blank_click(&ev) { on_blank.run(()) }>
                    {move || match tab.get() {
                        PlanTab::Gantt => view! {
                            <Gantt
                                scene=scene.into()
                                today=today
                                viewport=viewport
                                links=links
                                info=info
                                violation=violation
                                on_open=on_tap
                                on_edit=on_open
                                editing=chart_edit::editing(plan)
                                selected=Signal::derive(move || chosen.get())
                                on_toggle=on_toggle
                                on_blank=on_blank
                                reveal=reveal
                                jump=jump
                                focus=focus
                            />
                        }.into_any(),
                        PlanTab::List => view! { <List rows=rows on_open=on_open on_toggle=on_toggle /> }.into_any(),
                        PlanTab::Settings => view! { <PlanSettings plan=plan /> }.into_any(),
                    }}
                    {move || {
                        let (plan_node, at) = (chosen.get()?, bar_at.get()?);
                        (tab.get() == PlanTab::Gantt).then(|| view! {
                            <ContextBar
                                plan=plan
                                node=plan_node
                                left=at.0
                                top=at.1
                                on_edit=Callback::new(move |()| go(tab.get_untracked(), Some(plan_node.id())))
                                on_data=Callback::new(move |()| data_of.set(Some(plan_node)))
                            />
                        })
                    }}
                </div>
                {move || data_of.get().map(|data_node| view! {
                    <DataWindow plan=plan node=data_node on_close=Callback::new(move |()| data_of.set(None)) />
                })}
                {move || {
                    node.get()
                        .filter(|_| tab.get() != PlanTab::Settings)
                        .map(|node| view! { <ItemPane plan=plan node=node fresh=fresh guard=guard on_close=on_close /> })
                }}
                </div>
                <ConfirmLeave guard=guard />
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
// }}}

// AddMenu {{{
/// The plus button of the toolbar, which lists what can be added when the pointer is over it and
/// stays listed once it has been clicked
#[component]
fn AddMenu(on_add: Callback<Kind>) -> impl IntoView {
    let over = RwSignal::new(false);
    let pinned = RwSignal::new(false);
    crate::escape::on_escape(
        move || pinned.get() || over.get(),
        Callback::new(move |()| {
            pinned.set(false);
            over.set(false);
        }),
    );
    let shown = move || over.get() || pinned.get();
    let item = move |kind: Kind, icon: IconKind, label: &'static str, title: &'static str| {
        view! {
            <button
                type="button"
                class="additem"
                title=title
                aria-label=title
                on:click=move |_| {
                    over.set(false);
                    pinned.set(false);
                    on_add.run(kind);
                }
            >
                <Icon kind=icon size=20 />
                <span>{label}</span>
            </button>
        }
    };
    view! {
        // hovering opens it for a mouse only, a touch also sends a hover that nothing ends
        <div
            class="addmenu"
            on:pointerenter=move |ev| over.set(ev.pointer_type() == "mouse")
            on:pointerleave=move |_| over.set(false)
        >
            <button
                type="button"
                class="toolbar-btn"
                class:active=shown
                title="Add"
                aria-label="Add"
                aria-expanded=move || shown().to_string()
                on:click=move |_| pinned.update(|p| *p = !*p)
            >
                <Icon kind=IconKind::Plus />
            </button>
            <Show when=shown>
                <div class="addpop">
                    {item(Kind::Task, IconKind::AddTask, "Task", "Add task")}
                    {item(Kind::Milestone, IconKind::AddMilestone, "Milestone", "Add milestone")}
                    {item(Kind::Group, IconKind::AddGroup, "Group", "Add task group")}
                    {item(Kind::Buffer, IconKind::AddBuffer, "Buffer", "Add buffer")}
                </div>
            </Show>
            <Show when=move || pinned.get()>
                <div class="menu-backdrop" on:click=move |_| {
                    pinned.set(false);
                    over.set(false);
                }></div>
            </Show>
        </div>
    }
}
// }}}

// helpers {{{
/// Whether a click landed on nothing in particular, rather than on something that does its own thing
pub fn blank_click(ev: &leptos::ev::MouseEvent) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|t| {
            t.closest("button, a, input, select, canvas, .name, .pane")
                .ok()
                .flatten()
                .is_none()
        })
}

/// Puts the cursor in the first box of the pane's form, with what it holds selected
fn focus_first_box() {
    use wasm_bindgen::JsCast;
    let input = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector(".pane form input").ok().flatten())
        .and_then(|i| i.dyn_into::<web_sys::HtmlInputElement>().ok());
    if let Some(input) = input {
        let _ = input.focus();
        input.select();
    }
}
// }}}

// ItemPane {{{
/// The pane beside the chart for editing the item that is open
#[component]
pub fn ItemPane(
    plan: ShortId,
    node: PlanNode,
    fresh: RwSignal<Option<Uuid>>,
    guard: Guard,
    on_close: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    guard.opened();
    on_cleanup(move || guard.closed());
    // a new item has its first box ready to type in, its name or for a buffer its days
    if fresh.get_untracked() == Some(node.id()) {
        set_timeout(focus_first_box, std::time::Duration::from_millis(80));
    }
    let title = match node {
        PlanNode::Task(_) => "Edit Task",
        PlanNode::Milestone(_) => "Edit Milestone",
        PlanNode::PlanBuffer(_) => "Edit Buffer",
        PlanNode::PlanDelay(_) => "Edit Delay",
        PlanNode::TaskGroup(_) => "Edit Task Group",
    };
    let saved = Callback::new(move |()| {
        fresh.set(None);
        on_close.run(());
    });
    let cancel = Callback::new(move |()| {
        if fresh.get_untracked() == Some(node.id()) {
            fresh.set(None);
            remove_item(sync, plan, node);
        }
        on_close.run(());
    });
    // escape is the same as clicking off the pane: it asks first when something is unsaved
    crate::escape::on_escape(|| true, Callback::new(move |()| guard.then(cancel)));
    view! {
        <aside class="pane">
            <header>
                <h2>{title}</h2>
                <button type="button" class="plain" aria-label="Close" on:click=move |_| cancel.run(())>"✕"</button>
            </header>
            <div
                class="body"
                on:input=move |ev| guard.touched(&ev)
                on:change=move |ev| guard.touched(&ev)
                on:click=move |ev| guard.touched(&ev)
            >
                <ItemEditor plan=plan node=node on_close=saved on_cancel=cancel />
            </div>
        </aside>
    }
}
// }}}

// SearchWindow {{{
/// Filter the items by name, then go to one
#[component]
fn SearchWindow(
    plan: ShortId,
    on_found: Callback<PlanNode>,
    on_close: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let filter = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = input.get() {
            let _ = input.focus();
        }
    });
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
                node_ref=input
                placeholder="Filter tasks and milestones…"
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
                        PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => "▭",
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
// }}}

// UnplacedWindow {{{
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
// }}}

// List {{{
#[component]
fn List(
    rows: Memo<Vec<Row>>,
    on_open: Callback<PlanNode>,
    on_toggle: Callback<PlanNode>,
) -> impl IntoView {
    view! {
        <ul class="items">
            <For each=move || rows.get() key=|r| crate::ui::hashed(r) children=move |row| {
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
// }}}
