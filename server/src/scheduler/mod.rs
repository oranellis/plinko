// modules {{{
use std::{cmp::Reverse, collections::HashMap};

use chrono::Days;

use crate::{
    multitenancy::tenant::Tenant,
    scheduler::{
        ledger::{Ledger, MAX_FILL_DAYS},
        plan_solver::PlanSolver,
        state::TenantVolatileState,
    },
};
// }}}

pub mod fill;
pub mod ledger;
mod network;
mod plan_solver;
pub mod state;
pub mod status;

/// Caches the calendars a run reads: from the earliest day any plan has history on until the
/// search horizon. Call it before `solve_tenant` and after availability changes.
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
}

/// Schedules every plan in the tenant, from the calendars cached by `prepare`.
///
/// Plans share one ledger of people's hours and go in one after another, so an earlier plan takes
/// what it needs before the next sees what is left. The plan that finishes last when scheduled
/// alone goes first, then the one with the earlier start date, then the lower id.
pub fn solve_tenant(tenant: &Tenant) -> TenantVolatileState {
    let fresh = Ledger::from_tenant(tenant, tenant.today());
    let mut order: Vec<_> = tenant
        .plans()
        .values()
        .map(|plan| {
            let alone = PlanSolver::new(tenant, plan).solve(&mut fresh.clone());
            let start = *plan.scheduler_data().start_date();
            (Reverse(alone.end()), start, *plan.id())
        })
        .collect();
    order.sort();
    let mut ledger = fresh;
    let mut plans = HashMap::new();
    for (_, _, id) in order {
        let plan = &tenant.plans()[&id];
        plans.insert(id, PlanSolver::new(tenant, plan).solve(&mut ledger));
    }
    let mut state = TenantVolatileState::from_plans(plans);
    for (user, schedule) in &mut state.users {
        schedule.bottleneck = ledger.load(user);
    }
    state
}
