// modules {{{
use std::{borrow::Cow, cmp::Reverse, collections::HashMap};

use chrono::Days;

use crate::{
    multitenancy::tenant::Tenant,
    scheduler::{
        ledger::{Ledger, MAX_FILL_DAYS},
        plan_solver::PlanSolver,
        state::{Baseline, TenantVolatileState},
    },
    types::short_id::ShortId,
};
// }}}

mod combined;
pub mod fill;
pub mod ledger;
mod network;
mod plan_solver;
mod repair;
mod slack;
pub mod state;
pub mod status;

/// Caches what a run reads: calendars from the earliest day any plan has history on until the
/// search horizon, and the baseline of each plan that has none. Call it before `solve_tenant` and
/// after any update.
pub fn prepare(tenant: &mut Tenant) {
    let today = tenant.today();
    let plans = tenant.plans().values();
    let history = plans.flat_map(|p| {
        let starts = p
            .tasks()
            .values()
            .filter_map(|t| t.persistent_state().actual_start_date);
        starts.chain([*p.scheduler_data().start_date()])
    });
    let from = history.chain([today]).min().unwrap_or(today);
    tenant.refresh_calendars(from, today + Days::new(u64::from(MAX_FILL_DAYS)));
    let missing: Vec<_> = tenant
        .plans()
        .keys()
        .filter(|id| tenant.baseline(id).is_none())
        .copied()
        .collect();
    let solved: Vec<_> = missing
        .iter()
        .map(|id| (*id, baseline(tenant, id)))
        .collect();
    for (id, solved) in solved {
        tenant.store_baseline(id, solved);
    }
}

/// Schedules the tenant and keeps the result on it, for `Tenant::schedule`. The server runs this
/// after every update, `Tenant::apply_and_schedule` does.
pub fn schedule(tenant: &mut Tenant) {
    prepare(tenant);
    let state = solve_tenant(tenant);
    tenant.store_schedule(state);
}

/// The plan scheduled with every person's hours to itself
fn baseline(tenant: &Tenant, plan: &ShortId) -> Baseline {
    let mut ledger = Ledger::from_tenant(tenant, tenant.today());
    PlanSolver::new(tenant, &tenant.plans()[plan], None).solve_alone(&mut ledger)
}

/// Schedules every plan in the tenant, from what `prepare` cached.
///
/// Plans share one ledger of people's hours and are placed together, see `combined::solve`. Where
/// nothing else decides, the plan that finishes last when scheduled alone goes first, then the
/// one with the earlier start date, then the lower id. Each forecast is compared with the plan on
/// its own.
pub fn solve_tenant(tenant: &Tenant) -> TenantVolatileState {
    let baselines: HashMap<ShortId, Cow<Baseline>> = tenant
        .plans()
        .keys()
        .map(|id| {
            let cached = tenant.baseline(id).map(Cow::Borrowed);
            (
                *id,
                cached.unwrap_or_else(|| Cow::Owned(baseline(tenant, id))),
            )
        })
        .collect();
    let mut order: Vec<_> = tenant
        .plans()
        .values()
        .map(|plan| {
            let end = baselines[plan.id()].state.end();
            (
                Reverse(end),
                *plan.scheduler_data().start_date(),
                *plan.id(),
            )
        })
        .collect();
    order.sort();
    let ids: Vec<ShortId> = order.iter().map(|(_, _, id)| *id).collect();
    let outcome = repair::solve(tenant, &ids, &baselines);
    let plans = ids.into_iter().zip(outcome.states).collect();
    let mut state = TenantVolatileState::from_plans(plans);
    for (user, schedule) in &mut state.users {
        schedule.bottleneck = outcome.ledger.load(user);
    }
    state
}
