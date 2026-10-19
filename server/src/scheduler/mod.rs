// modules {{{
use std::{borrow::Cow, cmp::Reverse, collections::HashMap};

use chrono::{Days, NaiveDate};

use crate::{
    multitenancy::{tenant::Tenant, tenant_update::TenantUpdate},
    scheduler::{
        ledger::{Ledger, MAX_FILL_DAYS},
        plan_solver::PlanSolver,
        state::{Baseline, TenantVolatileState},
    },
    types::short_id::ShortId,
};
// }}}

// modules {{{
mod combined;
pub mod fill;
pub mod impact;
mod incremental;
pub mod ledger;
mod network;
mod plan_solver;
mod repair;
pub use repair::PlanRepairs;
mod slack;
pub mod state;
pub mod status;
// }}}

// fn prepare {{{
/// Caches what a run reads: calendars from the earliest day any plan has history on until the
/// search horizon, and the baseline of each plan that has none. Call it before `solve_tenant` and
/// after any update.
pub fn prepare(tenant: &mut Tenant) {
    let today = tenant.today();
    let plans = tenant.plans().values();
    let history = plans.flat_map(|p| {
        p.tasks()
            .values()
            .filter_map(|t| t.persistent_state().actual_start_date)
    });
    let from = history.chain([today]).min().unwrap_or(today);
    tenant.refresh_calendars(from, today + Days::new(u64::from(MAX_FILL_DAYS)));
    let missing: Vec<_> = tenant
        .plans()
        .keys()
        .filter(|id| tenant.baseline(id).is_none())
        .copied()
        .collect();
    // each plan on its own, so each on its own thread
    let solved: Vec<_> = std::thread::scope(|scope| {
        let running: Vec<_> = missing
            .iter()
            .map(|id| {
                let tenant = &*tenant;
                (*id, scope.spawn(move || baseline(tenant, id)))
            })
            .collect();
        running
            .into_iter()
            .map(|(id, r)| (id, r.join().expect("a schedule does not panic")))
            .collect()
    });
    for (id, solved) in solved {
        tenant.store_baseline(id, solved);
    }
}
// }}}

// fn schedule {{{
/// Schedules the tenant and keeps the result on it, for `Tenant::schedule`. The server runs this
/// after every update, `Tenant::apply_and_schedule` does.
pub fn schedule(tenant: &mut Tenant) -> Fixes {
    prepare(tenant);
    let (state, fixes) = solve_with_fixes(tenant);
    tenant.store_schedule(state);
    fixes
}
// }}}

/// What repair rounds changed in each plan, which a later run that places only some of the work
/// keeps so that it does not undo what they did
pub type Fixes = HashMap<ShortId, PlanRepairs>;

// fn plan_order {{{
/// The order plans are placed in: the one that finishes last on its own first, then by id
fn plan_order(tenant: &Tenant, end: impl Fn(&ShortId) -> Option<NaiveDate>) -> Vec<ShortId> {
    let mut order: Vec<_> = tenant
        .plans()
        .keys()
        .map(|id| (Reverse(end(id)), *id))
        .collect();
    order.sort();
    order.into_iter().map(|(_, id)| id).collect()
}
// }}}

// Fixes supporting types {{{
/// How a schedule was brought up to date
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refreshed {
    /// Nothing the updates changed is scheduled, so it is as it was
    Unchanged,
    /// Only what the updates changed and what depends on it was placed again
    Partly,
    /// Everything was scheduled again
    Fully,
}

/// Keeps a tenant's schedule up to date as updates arrive: apply updates through it, then ask for
/// the schedule. The schedule of the last run is kept, so a few changes cost placing those
/// changes, not placing everything again. It drifts from what a full run gives, so run `schedule`
/// now and then to bring it back.
#[derive(Default)]
pub struct Tracker {
    /// The schedule and baselines before the updates applied since the last run
    last: Option<(Option<TenantVolatileState>, HashMap<ShortId, Baseline>)>,
    change: impact::Change,
    /// How much trouble the schedule had when it was last worked out in full, which what is
    /// placed again may not get much past
    trouble: Option<i64>,
    fixes: Fixes,
}
// }}}

// constants {{{
/// How much worse than the last full run an incremental one may leave the schedule, half and
/// this much besides, before everything is scheduled again
const DRIFT: i64 = 30;
// }}}

// impl Tracker {{{
impl Tracker {
    /// Applies an update to the tenant, noting what it may change in the schedule
    pub fn apply(
        &mut self,
        tenant: &mut Tenant,
        update: TenantUpdate,
    ) -> crate::types::error::PlinkoResult<()> {
        let last = self
            .last
            .get_or_insert_with(|| (tenant.schedule().cloned(), tenant.baselines().clone()));
        let _ = last;
        let mut change = self.change.clone();
        change.note(tenant, &update.update_type);
        let applied = tenant.apply_update(update);
        if applied.is_ok() {
            self.change = change;
        }
        applied
    }

    /// Brings the tenant's schedule up to date and keeps it on the tenant
    pub fn schedule(&mut self, tenant: &mut Tenant) -> Refreshed {
        let change = std::mem::take(&mut self.change);
        let Some((previous, stale)) = self.last.take() else {
            // nothing applied since, so the schedule is the one on the tenant unless it has none
            if tenant.schedule().is_some() {
                return Refreshed::Unchanged;
            }
            return self.full(tenant);
        };
        // what is still right, which an update drops, comes back
        for (id, baseline) in stale {
            if tenant.plans().contains_key(&id) && tenant.baseline(&id).is_none() {
                tenant.store_baseline(id, baseline);
            }
        }
        let Some(previous) = previous else {
            return self.full(tenant);
        };
        if change.is_none() {
            tenant.store_schedule(previous);
            return Refreshed::Unchanged;
        }
        // a plan that changed is scheduled against its baseline from before, which is close
        // enough until a full run, but a plan with none is worked out
        prepare(tenant);
        let base = *self
            .trouble
            .get_or_insert_with(|| incremental::trouble(&previous));
        if let Some(state) = incremental::reschedule(tenant, &previous, &change, &self.fixes)
            && incremental::trouble(&state) <= base + DRIFT + base / 2
        {
            tenant.store_schedule(state);
            return Refreshed::Partly;
        }
        self.full(tenant)
    }

    /// Takes a schedule of everything on the tenant as the one later runs keep close to
    pub fn refined(&mut self, tenant: &Tenant, fixes: Fixes) {
        self.trouble = tenant.schedule().map(incremental::trouble);
        self.fixes = fixes;
    }

    /// Schedules everything, which the next runs may not drift far from
    fn full(&mut self, tenant: &mut Tenant) -> Refreshed {
        tenant.clear_baselines();
        let fixes = schedule(tenant);
        self.refined(tenant, fixes);
        Refreshed::Fully
    }
}
// }}}

// fn baseline {{{
/// The plan scheduled with every person's hours to itself
fn baseline(tenant: &Tenant, plan: &ShortId) -> Baseline {
    let mut ledger = Ledger::from_tenant(tenant, tenant.today());
    PlanSolver::new(tenant, &tenant.plans()[plan], None).solve_alone(&mut ledger)
}
// }}}

// fn solve_tenant {{{
/// Schedules every plan in the tenant, from what `prepare` cached.
///
/// Plans share one ledger of people's hours and are placed together, see `combined::solve`. Where
/// nothing else decides, the plan that finishes last when scheduled alone goes first, then the
/// one with the earlier start date, then the lower id. Each forecast is compared with the plan on
/// its own.
pub fn solve_tenant(tenant: &Tenant) -> TenantVolatileState {
    solve_with_fixes(tenant).0
}
// }}}

// fn solve_with_fixes {{{
fn solve_with_fixes(tenant: &Tenant) -> (TenantVolatileState, Fixes) {
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
    let ids = plan_order(tenant, |id| baselines[id].state.end());
    let outcome = repair::solve(tenant, &ids, &baselines);
    let fixes = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, outcome.repairs.of(i)))
        .collect();
    let plans = ids.into_iter().zip(outcome.states).collect();
    let mut state = TenantVolatileState::from_plans(plans);
    for (user, schedule) in &mut state.users {
        schedule.bottleneck = outcome.ledger.load(user);
    }
    (state, fixes)
}
// }}}
