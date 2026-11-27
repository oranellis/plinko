//! One plan: its toolbar of icons, its Gantt chart or list, and the window for the item that is open.

// modules {{{
use std::collections::HashSet;

use leptos::prelude::*;
use plinko::{
    common::short_id::ShortId,
    planner::items::{dependency::PlanNode, parent::Parent},
    scheduler::state::PlanVolatileState,
};
use uuid::Uuid;

use crate::{
    card, chart_edit,
    context::{ContextBar, InfoPanel},
    data::DataWindow,
    draft::{Draft, Pick},
    editor::{self, WorkersWindow},
    format,
    gantt::{self, Gantt, Staged, arrange::Mark, layout::Viewport, scene, shift::Held},
    guard::{ConfirmLeave, Guard},
    icons::{Icon, IconKind},
    model::{self, Look, Row},
    nav::{PlanTab, Route, navigate},
    settings::PlanSettings,
    ui::{Modal, use_sync},
};
// }}}

// types {{{
/// The kinds of item the add menu makes
#[derive(Clone, Copy)]
enum Kind {
    Task,
    Milestone,
    Group,
    Buffer,
    Delay,
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
    // an item being added, which is not in the plan until it is saved, and the group it goes in
    let adding = RwSignal::new(None::<(PlanNode, Parent)>);
    let node = Memo::new(move |_| {
        if let Some((node, _)) = adding.get() {
            return Some(node);
        }
        let id = open.get()?;
        sync.tenant.with(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            model::nodes(p).into_iter().find(|n| n.id() == id)
        })
    });
    // the item being created or edited as it would be saved, and as it was when it was opened
    let draft = RwSignal::new(None::<Draft>);
    let pristine = StoredValue::new(None::<Draft>);
    let probe: card::Probe = RwSignal::new(None);
    let spot = RwSignal::new(None::<PlanNode>);
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
    let revealed = reveal.get_untracked();
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
    let violation = Callback::new(move |mark: Mark| {
        let mut said = Vec::new();
        with_state(&mut |state| said = gantt::info::violation(state, mark));
        said
    });

    let selected = RwSignal::new(revealed);
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
    let go = move |tab: PlanTab, open: Option<Uuid>| {
        adding.set(None);
        navigate(Route::Plan(plan, tab, open))
    };
    // while an item of the plan is edited its bar is out of the way at the bottom of the chart,
    // and is there even when the item is not on the chart
    let edited = Memo::new(move |_| draft.with(|d| d.as_ref().filter(|d| !d.new).map(|d| d.node)));
    let editing_one = Memo::new(move |_| edited.get().is_some());
    let bar_of = Memo::new(move |_| edited.get().or_else(|| chosen.get()));
    // an item being created has the bar too, docked from the start
    let making = Memo::new(move |_| draft.with(|d| d.as_ref().is_some_and(|d| d.new)));
    let docked = Memo::new(move |_| editing_one.get() || making.get());
    let bar_shown = Memo::new(move |_| docked.get() || bar_at.with(Option::is_some));
    let hovered = RwSignal::new(None::<PlanNode>);
    let guard = Guard::new();
    // a click while a handle of the card is lit is the card's: it links, unlinks or places the
    // item being edited, and is nothing else. Gives whether it was.
    let card_click = move |clicked: Option<PlanNode>| {
        let (mut used, mut said) = (false, None);
        sync.tenant.with_untracked(|t| {
            let Some(p) = t.as_ref().and_then(|t| t.plans().get(&plan)) else {
                return;
            };
            draft.update(|d| {
                if let Some(d) = d {
                    match card::picked(p, d, clicked) {
                        Ok(card) => used = card,
                        Err(why) => (used, said) = (true, Some(why)),
                    }
                }
            });
        });
        if used {
            guard.dirtied();
        }
        if said.is_some() {
            sync.notice.set(said);
        }
        used
    };
    // what is being picked with the keyboard for an item that is only selected, which clicks
    // then change in the plan at once: what it waits for, what waits for it, or its group
    let live = RwSignal::new(None::<(PlanNode, Pick)>);
    let at_once = chart_edit::editing(plan);
    Effect::new(move |_| {
        let still = selected
            .get()
            .is_some_and(|s| live.with_untracked(|l| l.is_some_and(|l| l.0 == s)));
        if !still || draft.with(Option::is_some) {
            live.set(None);
        }
    });
    crate::escape::on_escape(
        move || live.with(Option::is_some),
        Callback::new(move |()| live.set(None)),
    );
    // a click while that is being picked is for it, and a click on nothing ends it. Gives
    // whether it was.
    let live_click = move |clicked: Option<PlanNode>| {
        let Some((own, pick)) = live.get_untracked() else {
            return false;
        };
        match (pick, clicked) {
            (Pick::Group, over) => {
                at_once.moved.run((own, over));
                live.set(None);
            }
            (_, None) => live.set(None),
            (_, Some(other)) if other == own => {}
            (_, Some(other)) => {
                let right = pick == Pick::After;
                let (waiting, before) = if right { (other, own) } else { (own, other) };
                let linked = sync.tenant.with_untracked(|t| {
                    let plan = t.as_ref().and_then(|t| t.plans().get(&plan));
                    plan.is_some_and(|p| model::dependencies(p, waiting).contains(&before))
                });
                match linked {
                    true => at_once.unlink.run((own, other)),
                    false => at_once.link.run((own, other, right)),
                }
            }
        }
        true
    };
    // an item is opened for editing, on the chart, or left only after asking about changes not
    // yet saved
    let on_open = Callback::new(move |node: PlanNode| {
        if card_click(Some(node)) {
            return;
        }
        guard.then(Callback::new(move |()| go(PlanTab::Gantt, Some(node.id()))));
    });
    // a tap on the chart selects, and with an item being edited that moves to the item tapped
    let on_tap = Callback::new(move |node: PlanNode| {
        if live_click(Some(node)) || card_click(Some(node)) {
            return;
        }
        selected.set(Some(node));
        if tab.get_untracked() == PlanTab::Gantt && open_node_exists.get_untracked() {
            on_open.run(node);
        }
    });
    let on_close = Callback::new(move |_| go(tab.get_untracked(), None));
    // the draft follows the item that is open, and what is selected follows an item of the plan
    Effect::new(move |_| {
        let open = node.get().filter(|_| tab.get() != PlanTab::Settings);
        let new = adding.get_untracked().map(|(_, parent)| parent);
        let next = open.and_then(|node| Draft::open(sync, plan, node, new));
        match &next {
            Some(next) => {
                guard.opened();
                if !next.new {
                    selected.set(Some(next.node));
                }
            }
            None => guard.closed(),
        }
        pristine.set_value(next.clone());
        draft.set(next);
    });
    // anything of the draft that is not as it was is a change to ask about, but not a lit handle
    Effect::new(move |_| {
        let now = draft.with(|d| d.clone().map(|d| Draft { pick: None, ..d }));
        match pristine.with_value(|was| *was == now) {
            true => guard.clean(),
            false => guard.dirtied(),
        }
    });
    let leave = Callback::new(move |()| {
        guard.clean();
        go(tab.get_untracked(), None);
    });
    let on_save = Callback::new(move |()| {
        if let Some(draft) = draft.get_untracked() {
            editor::save(sync, plan, &draft);
        }
        leave.run(());
    });
    let on_delete = Callback::new(move |()| {
        if let Some(node) = draft.with_untracked(|d| d.as_ref().map(|d| d.node)) {
            editor::remove_item(sync, plan, node);
        }
        selected.set(None);
        leave.run(());
    });
    guard.watch_clicks();
    on_key(
        "Enter",
        move || open_node_exists.try_get_untracked().unwrap_or(false) && !window_open(),
        on_save,
    );
    // the delete key asks to delete the item that is selected or edited, and enter then does
    let deleting = RwSignal::new(None::<PlanNode>);
    let delete = Callback::new(move |()| {
        let Some(node) = deleting.get_untracked() else {
            return;
        };
        deleting.set(None);
        editor::remove_item(sync, plan, node);
        selected.set(None);
        if draft.with_untracked(|d| d.as_ref().is_some_and(|d| d.node == node)) {
            leave.run(());
        }
    });
    on_key(
        "Delete",
        move || bar_of.try_get_untracked().flatten().is_some() && !window_open(),
        Callback::new(move |()| deleting.set(bar_of.get_untracked())),
    );
    on_key(
        "Enter",
        move || deleting.try_get_untracked().flatten().is_some(),
        delete,
    );
    // the task being created or edited, which has a card for who works on it
    let staffed = Memo::new(move |_| {
        draft.with(|d| {
            d.as_ref()
                .map(|d| d.node)
                .filter(|n| matches!(n, PlanNode::Task(_)))
        })
    });
    // escape is the same as clicking off the item: it asks first when something is unsaved
    crate::escape::on_escape(
        move || open_node_exists.get(),
        Callback::new(move |()| guard.then(on_close)),
    );
    let on_blank = Callback::new(move |()| {
        if live_click(None) || card_click(None) {
            return;
        }
        selected.set(None);
        if node.get_untracked().is_some() {
            guard.then(on_close);
        }
    });
    let on_toggle = Callback::new(move |node: PlanNode| {
        if live_click(Some(node)) || card_click(Some(node)) {
            return;
        }
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
            go(PlanTab::Gantt, Some(node.id()));
        }
    });

    // a new item starts as a draft on the card, in the group that is open or beside the item
    // that is, and is only added to the plan when it is saved
    let add = move |kind: Kind| {
        guard.then(Callback::new(move |()| {
            let open = node
                .get_untracked()
                .filter(|_| adding.get_untracked().is_none());
            let parent = sync.tenant.with_untracked(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                Some(card::beside(p, open))
            });
            // it has no id until the plan gives it one
            let nil = Uuid::nil();
            let new = match kind {
                Kind::Task => PlanNode::Task(nil),
                Kind::Milestone => PlanNode::Milestone(nil),
                Kind::Group => PlanNode::TaskGroup(nil),
                Kind::Buffer => PlanNode::PlanBuffer(nil),
                Kind::Delay => PlanNode::PlanDelay(nil),
            };
            go(PlanTab::Gantt, None);
            adding.set(Some((new, parent.unwrap_or(Parent::Plan))));
        }));
    };

    // letters, for what the buttons do. With the add menu listed they are the kinds of item,
    // with the day an item is held to picked they are how it is held, with an item edited they
    // light its handles, and with one only selected they edit it or pick for it at once.
    let menu = RwSignal::new(false);
    on_letter(move |key: char| {
        if window_open() || tab.try_get_untracked() != Some(PlanTab::Gantt) {
            return false;
        }
        if menu.get_untracked() {
            let kind = match key {
                't' => Kind::Task,
                'm' => Kind::Milestone,
                'g' => Kind::Group,
                'b' => Kind::Buffer,
                'd' => Kind::Delay,
                _ => return false,
            };
            menu.set(false);
            add(kind);
            return true;
        }
        let editing =
            draft.with_untracked(|d| d.as_ref().map(|d| (d.new, d.pick, d.pin().is_some())));
        if let Some((_, Some(Pick::Day), _)) = editing {
            let held = match key {
                'e' => Held::Earliest,
                'f' => Held::Fixed,
                'l' => Held::Latest,
                _ => return false,
            };
            draft.update(|d| {
                if let Some(d) = d {
                    d.hold_as(held)
                }
            });
            return true;
        }
        let light = |pick: Pick| {
            draft.update(|d| {
                if let Some(d) = d {
                    d.pick(pick)
                }
            })
        };
        match (key, editing, chosen.get_untracked()) {
            ('a', _, _) => menu.set(true),
            ('s', _, _) => searching.set(true),
            ('t', _, _) => jump.update(|n| *n += 1),
            ('d', Some(_), _) => light(Pick::Before),
            ('f', Some(_), _) => light(Pick::After),
            ('g', Some(_), _) => light(Pick::Group),
            // the day of an item of the plan is taken hold of, held first if it is not
            ('c', Some((false, _, held)), _) => {
                if !held {
                    crate::context::hold_next(sync, plan, draft);
                }
                light(Pick::Day);
            }
            ('e', None, Some(node)) => on_open.run(node),
            ('d' | 'f' | 'g', None, Some(node)) => {
                let pick = match key {
                    'd' => Pick::Before,
                    'f' => Pick::After,
                    _ => Pick::Group,
                };
                live.update(|l| *l = Some((node, pick)).filter(|next| *l != Some(*next)));
            }
            _ => return false,
        }
        true
    });
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
                            <AddMenu on_add=Callback::new(move |kind: Kind| add(kind)) pinned=menu />
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
                                violation=violation
                                on_open=on_tap
                                on_edit=on_open
                                editing=chart_edit::staging(chart_edit::editing(plan), plan, draft, guard)
                                selected=Signal::derive(move || chosen.get())
                                on_toggle=on_toggle
                                on_blank=on_blank
                                reveal=reveal
                                jump=jump
                                focus=focus
                                staged=Signal::derive(move || draft.with(|d| {
                                    d.as_ref().map(|d| {
                                        let group = match d.parent {
                                            Parent::Node(id) => Some(PlanNode::TaskGroup(id)),
                                            Parent::Plan => None,
                                        };
                                        // how far the days it really ran are from the ones it has
                                        let said: Vec<String> = [("start", d.moved.0), ("end", d.moved.1)]
                                            .into_iter()
                                            .filter(|(_, by)| *by != 0)
                                            .map(|(which, by)| format!("{which} {by:+}"))
                                            .collect();
                                        let days = Some(said.join("  ")).filter(|s| !s.is_empty());
                                        Staged {
                                            before: d.dependencies.clone(),
                                            after: d.dependents.clone(),
                                            group,
                                            item: (!d.new).then_some(d.node),
                                            days,
                                            pin: d.pin(),
                                            daying: d.pick == Some(Pick::Day),
                                            pick: match d.pick {
                                                Some(Pick::Before) => Some(false),
                                                Some(Pick::After) => Some(true),
                                                _ => None,
                                            },
                                        }
                                    })
                                }))
                                spot=spot
                                probe=probe
                                hovered=hovered
                                gripped=Signal::derive(move || live.get().map(|(node, pick)| {
                                    (node, match pick {
                                        Pick::Before => Some(false),
                                        Pick::After => Some(true),
                                        _ => None,
                                    })
                                }))
                            />
                        }.into_any(),
                        PlanTab::List => view! { <List rows=rows on_open=on_open on_toggle=on_toggle /> }.into_any(),
                        PlanTab::Settings => view! { <PlanSettings plan=plan /> }.into_any(),
                    }}
                    {move || {
                        // only made again for another item, so it slides when it is docked
                        let plan_node = bar_of.get().filter(|_| !making.get());
                        let shown = making.get() || plan_node.is_some() && bar_shown.get();
                        (tab.get() == PlanTab::Gantt && shown).then(|| view! {
                            <ContextBar
                                plan=plan
                                node=plan_node
                                at=bar_at
                                docked=docked
                                draft=draft
                                probe=probe
                                spot=spot
                                on_change=Callback::new(move |()| guard.dirtied())
                                on_edit=Callback::new(move |()| if let Some(node) = plan_node { on_open.run(node) })
                                on_data=Callback::new(move |()| data_of.set(plan_node))
                                on_save=on_save
                                on_cancel=leave
                                on_delete=on_delete
                            />
                        })
                    }}
                    // made again for each task, as it starts from what the task has
                    {move || {
                        let task = staffed.get().filter(|_| tab.get() == PlanTab::Gantt);
                        task.map(|_| view! { <WorkersWindow plan=plan draft=draft /> })
                    }}
                    <Show when=move || tab.get() == PlanTab::Gantt>
                        <card::Hint draft=draft live=Signal::derive(move || live.get().map(|l| l.1)) />
                        <InfoPanel
                            plan=plan
                            draft=draft
                            looked=Signal::derive(move || hovered.get().or_else(|| chosen.get()))
                            on_save=on_save
                        />
                    </Show>
                </div>
                {move || data_of.get().map(|data_node| view! {
                    <DataWindow plan=plan node=data_node on_close=Callback::new(move |()| data_of.set(None)) />
                })}
                </div>
                <ConfirmLeave guard=guard on_save=on_save />
                {move || deleting.get().map(|node| {
                    let name = sync.tenant.with_untracked(|t| {
                        let plan = t.as_ref().and_then(|t| t.plans().get(&plan));
                        plan.map(|p| model::node_name(p, node)).unwrap_or_default()
                    });
                    let keep = Callback::new(move |()| deleting.set(None));
                    // a click off it, or escape, is to keep it
                    view! {
                        <Modal title=format!("Delete {name}?") on_close=keep>
                            <div class="choices">
                                <button type="button" class="ctx" title="Keep it" aria-label="Keep it" on:click=move |_| keep.run(())>
                                    <Icon kind=IconKind::Close size=40 />
                                </button>
                                <button type="button" class="ctx cancel" title="Delete it" aria-label="Delete it" on:click=move |_| delete.run(())>
                                    <Icon kind=IconKind::Bin size=48 />
                                </button>
                            </div>
                        </Modal>
                    }
                })}
                <Show when=move || searching.get()>
                    <SearchWindow plan=plan on_found=on_found on_close=Callback::new(move |_| searching.set(false)) />
                </Show>
                <Show when=move || listing.get()>
                    <UnplacedWindow
                        scene=scene
                        on_open=Callback::new(move |node: PlanNode| {
                            listing.set(false);
                            go(PlanTab::Gantt, Some(node.id()));
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
fn AddMenu(
    on_add: Callback<Kind>,
    /// Whether it stays listed, which a key can make it too
    pinned: RwSignal<bool>,
) -> impl IntoView {
    let over = RwSignal::new(false);
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
                    {item(Kind::Delay, IconKind::AddDelay, "Delay", "Add delay")}
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
/// Whether a window is open over the page
fn window_open() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector(".modal").ok().flatten())
        .is_some()
}

/// A key that does something to the page while `active`, from anywhere it is not something
/// else: typed into a box, or pressed on a button outside the bar
fn on_key(key: &'static str, active: impl Fn() -> bool + 'static, run: Callback<()>) {
    use wasm_bindgen::{JsCast, closure::Closure};
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() != key || ev.default_prevented() || !active() {
                return;
            }
            let target = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            // a button of the bar that was last clicked still has the keyboard, and is not what
            // the key is for: enter would press it again
            let taken = target.is_some_and(|t| {
                let within = |what: &str| t.closest(what).ok().flatten().is_some();
                within("input, textarea, select, a") || within("button") && !within(".contextbar")
            });
            if !taken {
                ev.prevent_default();
                run.run(());
            }
        });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
}

/// A letter typed at the page and not into anything, with nothing held down with it. `run`
/// gives whether it was for something.
fn on_letter(run: impl Fn(char) -> bool + 'static) {
    use wasm_bindgen::{JsCast, closure::Closure};
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            let held = ev.ctrl_key() || ev.meta_key() || ev.alt_key();
            let key = ev.key();
            let mut letters = key.chars();
            let (Some(letter), None, false) = (letters.next(), letters.next(), held) else {
                return;
            };
            let target = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            let typing = target.is_some_and(|t| {
                t.closest("input, textarea, select")
                    .ok()
                    .flatten()
                    .is_some()
            });
            if !typing && !ev.default_prevented() && run(letter.to_ascii_lowercase()) {
                ev.prevent_default();
            }
        });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
}

/// Whether a click landed on nothing in particular, rather than on something that does its own thing
pub fn blank_click(ev: &leptos::ev::MouseEvent) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|t| {
            t.closest(
                "button, a, input, select, canvas, .name, .contextbar, .infopanel, .workerwin",
            )
            .ok()
            .flatten()
            .is_none()
        })
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
