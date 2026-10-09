//! Every task you are down for, across all plans, as rows on a chart in the order they start.

use std::collections::HashMap;

use leptos::prelude::*;
use plinko::{
    scheduler::state::PlanVolatileState,
    types::{dependency::PlanNode, short_id::ShortId},
};
use uuid::Uuid;

use crate::{
    gantt::{
        Gantt,
        arrange::{Entry, Kind, Mark, arrange_rows},
        info::info,
        layout::Viewport,
        links::Link,
        scene::Scene,
    },
    icons::{Icon, IconKind},
    mine::{MyTask, use_my_tasks},
    model::Look,
    plan::ItemPane,
    ui::use_sync,
};

#[component]
pub fn Allocations() -> impl IntoView {
    let sync = use_sync();
    let mine = use_my_tasks();
    let today = Signal::derive(move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default())
    });
    // where each task lives, to find its plan from the chart
    let plans = Memo::new(move |_| {
        mine.with(|m| {
            m.iter()
                .map(|t| (t.task, t.clone()))
                .collect::<HashMap<Uuid, MyTask>>()
        })
    });
    let scene = Memo::new(move |_| {
        let entries: Vec<Entry> = mine.with(|m| m.iter().map(entry).collect());
        let first = mine.with(|m| m.iter().map(|t| t.start).min());
        let plan_start = first.map_or(today.get(), |f| f.min(today.get()));
        let arrangement = arrange_rows(&entries);
        let last = mine
            .with(|m| m.iter().map(|t| t.end).max())
            .unwrap_or(plan_start);
        Scene {
            plan_start,
            arrangement,
            unplaced: Vec::new(),
            last_day: (last - plan_start).num_days() as f64 + 1.0,
        }
    });
    let viewport = RwSignal::new(None::<Viewport>);
    let jump = RwSignal::new(0u32);
    let focus = RwSignal::new(None::<Mark>);
    let open = RwSignal::new(None::<(ShortId, PlanNode)>);
    let fresh = RwSignal::new(None::<Uuid>);
    let info = Callback::new(move |mark: Mark| {
        let Mark::Node(node) = mark else {
            return Vec::new();
        };
        let Some(mine) = plans.with(|p| p.get(&node.id()).cloned()) else {
            return Vec::new();
        };
        let state: Option<PlanVolatileState> = sync.schedule.with(|s| {
            s.as_ref()
                .and_then(|(_, s)| s.plans.get(&mine.plan).cloned())
        });
        let mut lines = sync
            .tenant
            .with(|t| {
                let t = t.as_ref()?;
                Some(info(t, t.plans().get(&mine.plan)?, state.as_ref(), mark))
            })
            .unwrap_or_default();
        // the title first, then which plan it is from
        lines.insert(1.min(lines.len()), format!("Plan: {}", mine.plan_name));
        lines
    });
    let on_open = Callback::new(move |node: PlanNode| {
        if let Some(mine) = plans.with_untracked(|p| p.get(&node.id()).cloned()) {
            open.set(Some((mine.plan, node)));
        }
    });
    view! {
        <div class="plan">
            <div class="toolbar">
                <div class="toolbar-buttons">
                    <div class="toolbar-left">
                        <button
                            type="button"
                            class="toolbar-btn"
                            title="Jump to today"
                            aria-label="Jump to today"
                            on:click=move |_| jump.update(|n| *n += 1)
                        >
                            <Icon kind=IconKind::Today />
                        </button>
                    </div>
                    <span class="toolbar-title"><span class="toolbar-title-text">"Allocations"</span></span>
                </div>
            </div>
            <div class="workspace">
                <div class="main">
                    <Show
                        when=move || mine.with(|m| !m.is_empty())
                        fallback=|| view! { <p class="empty">"No tasks are scheduled for you."</p> }
                    >
                        <Gantt
                            scene=scene.into()
                            today=today
                            viewport=viewport
                            links=Callback::new(|_| Vec::<Link>::new())
                            info=info
                            violation=Callback::new(|_| Vec::new())
                            on_open=on_open
                            on_toggle=Callback::new(|_| {})
                            jump=jump
                            focus=focus
                        />
                    </Show>
                </div>
                {move || {
                    open.get().map(|(plan, node)| view! {
                        <ItemPane
                            plan=plan
                            node=node
                            fresh=fresh
                            on_close=Callback::new(move |_| open.set(None))
                        />
                    })
                }}
            </div>
        </div>
    }
}

fn entry(task: &MyTask) -> Entry {
    Entry {
        node: PlanNode::Task(task.task),
        name: task.name.clone(),
        kind: Kind::Task,
        span: (task.start, task.end + chrono::Days::new(1)),
        look: Look::from(task.status),
        violated: false,
        leaf: false,
        target: false,
        members: Vec::new(),
        collapsed: false,
    }
}
