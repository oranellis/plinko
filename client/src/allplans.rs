//! Every visible plan on one read-only chart; clicking an item opens it in its plan.

// modules {{{
use std::collections::HashSet;

use leptos::prelude::*;
use plinko::{
    common::short_id::ShortId, planner::items::dependency::PlanNode,
    scheduler::state::PlanVolatileState,
};
use uuid::Uuid;

use crate::{
    context::InfoPanel,
    draft::Draft,
    gantt::{
        self, Gantt,
        arrange::Mark,
        layout::Viewport,
        scene::{self, Scene, Shown},
        shift::Editing,
    },
    icons::{Icon, IconKind},
    nav::{PlanTab, Reveal, Route, navigate},
    ui::use_sync,
    views::{ViewButton, load_of, people, use_view},
};
// }}}

#[component]
pub fn AllPlans() -> impl IntoView {
    let sync = use_sync();
    let (view, set_view) = use_view("all".to_string());
    let me = Memo::new(move |_| sync.me.with(|m| m.as_ref().map(|m| m.id)));
    let collapsed = RwSignal::new(HashSet::<Uuid>::new());
    let today = Signal::derive(move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default())
    });
    let scene = Memo::new(move |_| {
        let options = view.get();
        sync.schedule.with(|schedule| {
            let schedule = schedule.as_ref().map(|(_, s)| s);
            collapsed.with(|collapsed| {
                sync.tenant.with(|t| {
                    let Some(t) = t.as_ref() else {
                        return Scene::empty(today.get_untracked());
                    };
                    let mut shown: Vec<Shown> = t
                        .plans()
                        .iter()
                        .map(|(id, plan)| Shown {
                            id: *id,
                            plan,
                            state: schedule.and_then(|s| s.plans.get(id)),
                        })
                        .collect();
                    shown.sort_by(|a, b| (a.plan.name(), a.id).cmp(&(b.plan.name(), b.id)));
                    let split = options.split.then(|| people(t, &shown, me.get(), true));
                    let load = load_of(t, schedule);
                    let options = scene::Options {
                        boxed: true,
                        only: me.get().filter(|_| options.mine),
                        split,
                        load: &load,
                    };
                    scene::compose(&shown, &options, collapsed, t.today())
                })
            })
        })
    });
    let owner = move |mark: Mark| scene.with_untracked(|s| s.owners.get(&mark.id()).copied());
    let with_plan =
        move |mark: Mark, f: &dyn Fn(&plinko::planner::plan::Plan, Option<&PlanVolatileState>)| {
            let Some(id) = owner(mark) else {
                return;
            };
            sync.schedule.with_untracked(|schedule| {
                sync.tenant.with_untracked(|t| {
                    if let Some(plan) = t.as_ref().and_then(|t| t.plans().get(&id)) {
                        f(plan, schedule.as_ref().and_then(|(_, s)| s.plans.get(&id)));
                    }
                })
            })
        };
    let links = Callback::new(move |mark: Mark| {
        let found = std::cell::RefCell::new(Vec::new());
        with_plan(mark, &|plan, state| {
            *found.borrow_mut() = gantt::links::links(plan, state, mark);
        });
        found.into_inner()
    });
    let violation = Callback::new(move |mark: Mark| {
        let said = std::cell::RefCell::new(Vec::new());
        with_plan(mark, &|_, state| {
            *said.borrow_mut() = gantt::info::violation(state, mark);
        });
        said.into_inner()
    });
    let reveal = expect_context::<Reveal>().0;
    let open = Callback::new(move |node: PlanNode| {
        let Some(plan) = owner(node) else {
            return;
        };
        if node.id() != scene::plan_mark(plan) {
            reveal.set(Some((plan, node.id())));
        }
        navigate(Route::Plan(plan, PlanTab::Gantt, None));
    });
    let on_toggle = Callback::new(move |node: PlanNode| {
        collapsed.update(|c| {
            if !c.remove(&node.id()) {
                c.insert(node.id());
            }
        })
    });
    let viewport = RwSignal::new(None::<Viewport>);
    let jump = RwSignal::new(0u32);
    let focus = RwSignal::new(None::<Mark>);
    let hovered = RwSignal::new(None::<PlanNode>);
    let looked_plan = Memo::new(move |_| {
        let node = hovered.get()?;
        let plan = scene.with(|s| s.owners.get(&node.id()).copied())?;
        (node.id() != scene::plan_mark(plan)).then_some((plan, node))
    });
    let plan_of_look = Memo::new(move |_| looked_plan.get().map(|(p, _)| p));
    view! {
        <div class="plan">
            <div class="toolbar">
                <div class="toolbar-buttons">
                    <div class="toolbar-left">
                        <button type="button" class="toolbar-btn" title="Back to plans" aria-label="Back to plans" on:click=move |_| navigate(Route::Plans)>
                            <Icon kind=IconKind::Back />
                        </button>
                        <button type="button" class="toolbar-btn" title="Jump to today" aria-label="Jump to today" on:click=move |_| jump.update(|n| *n += 1)>
                            <Icon kind=IconKind::Today />
                        </button>
                    </div>
                    <span class="toolbar-title">
                        <span class="toolbar-title-text">"All plans"</span>
                    </span>
                    <div class="toolbar-right">
                        <ViewButton options=view on_change=set_view />
                    </div>
                </div>
            </div>
            <div class="workspace">
                <div class="main">
                    <Gantt
                        scene=scene.into()
                        today=today
                        viewport=viewport
                        links=links
                        violation=violation
                        on_open=open
                        on_edit=open
                        editing=Editing::none()
                        selected=Signal::derive(|| None)
                        on_toggle=on_toggle
                        jump=jump
                        focus=focus
                        hovered=hovered
                        readonly=true
                    />
                    {move || plan_of_look.get().map(|plan: ShortId| view! {
                        <InfoPanel
                            plan=plan
                            draft=RwSignal::new(None::<Draft>)
                            looked=Signal::derive(move || looked_plan.get().map(|(_, n)| n))
                            on_save=Callback::new(|()| {})
                        />
                    })}
                </div>
            </div>
        </div>
    }
}
