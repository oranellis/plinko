// modules {{{
use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    common::short_id::ShortId,
    multitenancy::tenant_update::{PlanTenantUpdate, TenantUpdateType},
    planner::plan::NewPlan,
    protocol::Created,
};

use crate::{
    format,
    nav::Route,
    shell::open_plan,
    ui::{Actions, Field, Modal, Text, use_sync},
};
// }}}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Card {
    pub id: ShortId,
    pub name: String,
    /// Tasks, groups, milestones, buffers and delays
    items: usize,
    /// Latest forecast among the plan's targets
    last_target: Option<NaiveDate>,
    problems: usize,
    /// Starred by the signed-in person
    pub favourite: bool,
}

// use_cards {{{
pub fn use_cards() -> Memo<Vec<Card>> {
    let sync = use_sync();
    Memo::new(move |_| {
        let me = sync.me.with(|m| m.as_ref().map(|m| m.id));
        sync.schedule.with(|schedule| {
            sync.tenant.with(|tenant| {
                let Some(tenant) = tenant else {
                    return Vec::new();
                };
                let starred = me
                    .and_then(|me| tenant.users().get(&me))
                    .map(|u| u.favourites().clone())
                    .unwrap_or_default();
                let mut cards: Vec<Card> = tenant
                    .plans()
                    .values()
                    .map(|plan| {
                        let state = schedule.as_ref().and_then(|(_, s)| s.plans.get(plan.id()));
                        Card {
                            id: *plan.id(),
                            name: plan.name().to_string(),
                            items: plan.tasks().len()
                                + plan.task_groups().len()
                                + plan.milestones().len()
                                + plan.plan_buffers().len()
                                + plan.plan_delays().len(),
                            last_target: state
                                .filter(|_| !plan.targets().is_empty())
                                .and_then(|s| s.targets.values().map(|t| t.forecast).max()),
                            problems: state
                                .map_or(0, |s| s.unscheduled.len() + s.constraint_violations.len()),
                            favourite: starred.contains(plan.id()),
                        }
                    })
                    .collect();
                cards.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
                cards
            })
        })
    })
}
// }}}

/// A non-breaking space when there is no target, so every card has the same lines
fn target_line(last: Option<NaiveDate>) -> String {
    last.map_or("\u{a0}".to_string(), |end| {
        format!("Last Target Finishes: {}", format::day(end))
    })
}

// PlanCards {{{
#[component]
pub fn PlanCards(
    #[prop(into)] cards: Signal<Vec<Card>>,
    #[prop(optional, into)] all: Option<Signal<Vec<Card>>>,
) -> impl IntoView {
    let sync = use_sync();
    view! {
        <ul class="cards">
            {move || all.map(|all| all.get()).filter(|all| !all.is_empty()).map(|all| {
                let items: usize = all.iter().map(|c| c.items).sum();
                view! {
                    <li>
                        <a class="card allcard" href=Route::AllPlans.path()>
                            <strong>"All plans"</strong>
                            <span>{format!("{items} items in {} plans", all.len())}</span>

                        </a>
                    </li>
                }
            })}
            <For each=move || cards.get() key=|c| crate::ui::hashed(c) children=move |card| {
                let id = card.id;
                let starred = card.favourite;
                view! {
                    <li>
                        <a class="card" href=Route::Plan(id, crate::nav::PlanTab::Gantt, None).path()>
                            <strong>{card.name.clone()}</strong>
                            <span>{match card.items {
                                1 => "1 item".to_string(),
                                n => format!("{n} items"),
                            }}</span>
                            <span>{target_line(card.last_target)}</span>
                            {(card.problems > 0).then(|| view! {
                                <span class="problem">{format!("⚠ {} need attention", card.problems)}</span>
                            })}
                        </a>
                        <button
                            type="button"
                            class="star"
                            class:on=starred
                            title=if starred { "Remove from favourites" } else { "Add to favourites" }
                            aria-pressed=starred.to_string()
                            on:click=move |_| sync.send(TenantUpdateType::Favourite(id, !starred))
                        >
                            "★"
                        </button>
                    </li>
                }
            } />
        </ul>
    }
}
// }}}

// Plans {{{
#[component]
pub fn Plans() -> impl IntoView {
    let cards = use_cards();
    let favourites = Memo::new(move |_| {
        cards.with(|c| {
            c.iter()
                .filter(|c| c.favourite)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let others = Memo::new(move |_| {
        cards.with(|c| {
            c.iter()
                .filter(|c| !c.favourite)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let creating = RwSignal::new(false);
    view! {
        <div class="bar">
            <h1>"Plans"</h1>
            <button type="button" class="primary" on:click=move |_| creating.set(true)>"New Plan"</button>
        </div>
        <Show when=move || cards.with(Vec::is_empty)>
            <div class="empty">
                <img src="plinko_logo.svg" alt="" />
                <p class="title">"Plinko"</p>
                <p>"You are not in any plans yet. Make one to get started."</p>
            </div>
        </Show>
        <Show when=move || favourites.with(|f| !f.is_empty())>
            <h2 class="section">"Favourites"</h2>
            <PlanCards cards=favourites />
            <h2 class="section">"Other Plans"</h2>
        </Show>
        <PlanCards cards=others all=cards />
        <Show when=move || creating.get()>
            <NewPlanForm on_close=Callback::new(move |_| creating.set(false)) />
        </Show>
    }
}
// }}}

// NewPlanForm {{{
#[component]
fn NewPlanForm(on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let name = RwSignal::new(String::new());
    let make = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if name.with_untracked(|n| n.trim().is_empty()) {
            return;
        }
        let new = NewPlan {
            id: ShortId::new(),
            name: name.get_untracked(),
        };
        sync.send_then(
            TenantUpdateType::Plan(PlanTenantUpdate::Add(new)),
            move |created| {
                if let Some(Created::Plan(plan)) = created {
                    open_plan(plan);
                }
            },
        );
        on_close.run(());
    };
    view! {
        <Modal title="New Plan" on_close=on_close>
            <form class="form" on:submit=make>
                <Field label="Plan Name">
                    <Text value=name on_input=Callback::new(move |v| name.set(v)) placeholder="My Plan" />
                </Field>
                <Actions on_cancel=on_close />
            </form>
        </Modal>
    }
}
// }}}
