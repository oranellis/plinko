//! Editing when people work: the rules for a person, or for the placeholders of a plan, and the
//! days an affinity allows. The rules are the ones the scheduler uses, so the calendar shown is
//! what it will schedule against. Both a full page and a window over the current page.

// modules {{{
use leptos::prelude::*;
use plinko::{
    availability::rule_set::{RuleSet, RuleSetUpdate},
    availability::workload::Workload,
    common::short_id::ShortId,
    multitenancy::tenant_update::TenantUpdateType,
    planner::plan_update::PlanUpdate,
};
use uuid::Uuid;

use crate::{
    nav::{Availability as Of, PlanTab, Route},
    rulecalendar::RuleCalendar,
    ui::{Back, Modal, use_sync},
};
// }}}

// hours_of {{{
/// The hours rules of a person or plan, and a way to change them
fn hours_of(
    of: Of,
) -> (
    Signal<Option<RuleSet<Workload>>>,
    Callback<RuleSetUpdate<Workload>>,
) {
    let sync = use_sync();
    let rules = Memo::new(move |_| {
        sync.tenant.with(|t| {
            let t = t.as_ref()?;
            match of {
                Of::User(id) => t.users().get(&id).map(|u| u.availability().clone()),
                Of::Plan(id) => t.plans().get(&id).map(|p| p.default_availability().clone()),
            }
        })
    });
    let send = Callback::new(move |update: RuleSetUpdate<Workload>| match of {
        Of::User(id) => sync.send(TenantUpdateType::Availability(id, update)),
        Of::Plan(id) => sync.plan(id, PlanUpdate::Availability(update)),
    });
    (rules.into(), send)
}
// }}}

// title_of {{{
fn title_of(of: Of) -> Signal<String> {
    let sync = use_sync();
    Signal::derive(move || {
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                Some(match of {
                    Of::User(id) => format!("Hours of {}", t.users().get(&id)?.name()),
                    Of::Plan(id) => {
                        format!("Default Availability of {}", t.plans().get(&id)?.name())
                    }
                })
            })
            .unwrap_or_default()
    })
}
// }}}

// Availability {{{
/// The calendar as a page of its own, for changing your hours
#[component]
pub fn Availability(of: Of) -> impl IntoView {
    let (rules, send) = hours_of(of);
    let me = use_sync().me.get_untracked().map(|m| m.id);
    // your own hours are a main page, other people's and a plan's are reached from somewhere else
    let back = match of {
        Of::User(id) if Some(id) == me => None,
        Of::User(_) => Some(Route::People),
        Of::Plan(id) => Some(Route::Plan(id, PlanTab::Settings, None)),
    };
    view! {
        <div class="calpage">
            {back.map(|to| view! { <Back to=to /> })}
            <div class="bar">
                <h1>{title_of(of)}</h1>
            </div>
            <Show when=move || rules.with(Option::is_some) fallback=|| view! { <p>"Not found."</p> }>
                <RuleCalendar rules=rules send=send />
            </Show>
        </div>
    }
}
// }}}

// AvailabilityWindow {{{
/// The calendar in a window over the page it was opened from
#[component]
pub fn AvailabilityWindow(of: Of, on_close: Callback<()>) -> impl IntoView {
    let (rules, send) = hours_of(of);
    view! {
        <Modal title=title_of(of) on_close=on_close wide=true>
            <RuleCalendar rules=rules send=send />
        </Modal>
    }
}
// }}}

// AffinityWindow {{{
/// The days an affinity lets work happen on, in a window
#[component]
pub fn AffinityWindow(plan: ShortId, affinity: Uuid, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let rules = Signal::derive(move || {
        sync.tenant.with(|t| {
            let a = t
                .as_ref()?
                .plans()
                .get(&plan)?
                .affinities()
                .get(&affinity)?;
            Some(a.rules().clone())
        })
    });
    let send = Callback::new(move |update: RuleSetUpdate<bool>| {
        sync.plan(plan, PlanUpdate::AffinityRules(affinity, update));
    });
    let title = Signal::derive(move || {
        sync.tenant
            .with(|t| {
                let a = t
                    .as_ref()?
                    .plans()
                    .get(&plan)?
                    .affinities()
                    .get(&affinity)?;
                Some(format!("Days for {}", a.name()))
            })
            .unwrap_or_default()
    });
    view! {
        <Modal title=title on_close=on_close wide=true>
            <RuleCalendar rules=rules send=send />
        </Modal>
    }
}
// }}}
