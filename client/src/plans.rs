//! The plans you can see, with how they are doing.

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    multitenancy::tenant_update::{PlanTenantUpdate, TenantUpdateType},
    planner::plan::NewPlan,
    protocol::Created,
    types::short_id::ShortId,
};

use crate::{
    format,
    nav::Route,
    shell::open_plan,
    ui::{Field, Modal, Text, use_sync},
};

#[derive(Clone, Debug, PartialEq)]
struct Card {
    id: ShortId,
    name: String,
    start: NaiveDate,
    tasks: usize,
    /// When it finishes and how late that is compared with the plan on its own
    forecast: Option<(NaiveDate, i64)>,
    problems: usize,
}

#[component]
pub fn Plans() -> impl IntoView {
    let sync = use_sync();
    let cards = Memo::new(move |_| {
        let schedule = sync.schedule.get();
        sync.tenant.with(|tenant| {
            let Some(tenant) = tenant else {
                return Vec::new();
            };
            let mut cards: Vec<Card> = tenant
                .plans()
                .values()
                .map(|plan| {
                    let state = schedule.as_ref().and_then(|(_, s)| s.plans.get(plan.id()));
                    Card {
                        id: *plan.id(),
                        name: plan.name().to_string(),
                        start: *plan.scheduler_data().start_date(),
                        tasks: plan.tasks().len(),
                        forecast: state
                            .and_then(|s| s.plan_end)
                            .map(|e| (e.forecast, e.delay())),
                        problems: state
                            .map_or(0, |s| s.unscheduled.len() + s.constraint_violations.len()),
                    }
                })
                .collect();
            cards.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
            cards
        })
    });
    let creating = RwSignal::new(false);
    view! {
        <div class="bar">
            <h1>"Plans"</h1>
            <button type="button" class="primary" on:click=move |_| creating.set(true)>"New plan"</button>
        </div>
        <Show when=move || cards.with(Vec::is_empty)>
            <div class="empty">
                <img src="plinko_logo.svg" alt="" />
                <p class="title">"Plinko"</p>
                <p>"You are not in any plans yet. Make one to get started."</p>
            </div>
        </Show>
        <ul class="cards">
            <For each=move || cards.get() key=|c| format!("{c:?}") children=move |card| {
                let id = card.id;
                view! {
                    <li>
                        <a class="card" href=Route::Plan(id, crate::nav::PlanTab::Gantt, None).path()>
                            <strong>{card.name.clone()}</strong>
                            <span>{format!("{} tasks · starts {}", card.tasks, format::day(card.start))}</span>
                            {card.forecast.map(|(end, late)| view! {
                                <span class:late={late > 0}>
                                    {format!("Finishes {} · {}", format::day(end), format::delay(late))}
                                </span>
                            })}
                            {(card.problems > 0).then(|| view! {
                                <span class="problem">{format!("⚠ {} need attention", card.problems)}</span>
                            })}
                        </a>
                    </li>
                }
            } />
        </ul>
        <Show when=move || creating.get()>
            <NewPlanForm on_close=Callback::new(move |_| creating.set(false)) />
        </Show>
    }
}

#[component]
fn NewPlanForm(on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let name = RwSignal::new(String::new());
    let start = RwSignal::new(
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()))
            .unwrap_or_default(),
    );
    let make = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let new = NewPlan {
            id: ShortId::new(),
            name: name.get_untracked(),
            start_date: start.get_untracked(),
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
        <Modal title="New plan" on_close=on_close>
            <form on:submit=make>
                <Field label="Name">
                    <Text value=name on_input=Callback::new(move |v| name.set(v)) placeholder="Website relaunch" />
                </Field>
                <Field label="Starts">
                    <input
                        type="date"
                        prop:value=move || format::iso(start.get())
                        on:input=move |ev| {
                            if let Some(day) = format::parse_iso(&event_target_value(&ev)) {
                                start.set(day);
                            }
                        }
                    />
                </Field>
                <button type="submit" class="primary" disabled=move || name.with(|n| n.trim().is_empty())>
                    "Create"
                </button>
            </form>
        </Modal>
    }
}
