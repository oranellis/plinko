//! The scheduler. A tenant is lowered to one network: vertices are day boundaries ("these
//! things are done") and arcs are what takes days between them. Arcs are placed one at a time
//! as early as they fit, the most urgent first, then contested choices are tried the other way
//! and the best schedule is kept. See `docs/scheduler.md`.

// modules {{{
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::short_id::ShortId,
    multitenancy::tenant::Tenant,
    planner::items::{dependency::PlanNode, task::Worker},
    scheduler::{
        arc::ArcId,
        network::Network,
        objective::Late,
        pass::{Context, Overrides},
        state::TenantVolatileState,
        table::{RowKey, Table},
    },
};
// }}}

pub mod arc;
pub mod bits;
pub mod bound;
pub mod day;
pub mod justify;
pub mod ledger;
pub mod lower;
pub mod network;
pub mod objective;
pub mod pass;
pub mod search;
pub mod state;
pub mod status;
pub mod table;
pub mod write;

// constants {{{
/// The table covers the work in the tenant end to end this many times over past today
const HORIZON_FACTOR: u32 = 4;

/// The fewest and the most days past today a table covers
const HORIZON: (u32, u32) = (120, 3650);
// }}}

/// A schedule and the choices worth starting the next search from
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solved {
    pub state: TenantVolatileState,
    pub hints: Hints,
}

// Solved supporting types {{{
/// What a search found better done differently, by the ids of what it is about so it outlives
/// the run
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hints {
    /// The first waits until the second is placed
    pub after: Vec<(Item, Item)>,
    /// The person a worker of a task resolves to
    pub people: Vec<(Item, Worker, Uuid)>,
    /// The hard date of a node counts as this many days sooner when deciding what goes first
    pub sooner: Vec<(Item, i64)>,
}

/// A node of a plan
pub type Item = (ShortId, PlanNode);
// }}}

// schedule {{{
/// Schedules the tenant and keeps the result on it, for `Tenant::schedule`. The search starts
/// from the hints the last schedule left.
pub fn schedule(tenant: &mut Tenant) {
    let hints = tenant.hints().clone();
    let solved = solve(tenant, &hints);
    tenant.store_schedule(solved.state);
    tenant.store_hints(solved.hints);
}
// }}}

// solve {{{
/// Schedules the tenant, starting from the hints of the last schedule. If work runs past the
/// days looked at, more are looked at.
pub fn solve(tenant: &mut Tenant, hints: &Hints) -> Solved {
    let today = tenant.today();
    let tasks = || tenant.plans().values().flat_map(|p| p.tasks().values());
    let history = tasks().filter_map(|t| t.persistent_state().actual_start_date);
    let origin = history.chain([today]).min().unwrap_or(today);
    let waits = tenant.plans().values().flat_map(|p| {
        let delays = p.plan_delays().values().map(|d| *d.duration());
        delays.chain(p.plan_buffers().values().map(|b| *b.duration()))
    });
    let work: u32 = tasks().map(|t| t.duration()).chain(waits).sum();
    let mut ahead = (work * HORIZON_FACTOR).clamp(HORIZON.0, HORIZON.1);
    loop {
        let (solved, overflow) = solve_over(tenant, hints, origin, ahead);
        if !overflow || ahead >= HORIZON.1 {
            return solved;
        }
        ahead = (ahead * 2).min(HORIZON.1);
    }
}

/// Schedules over the days from `origin` to `ahead` days past today, and says whether work ran
/// out of days that more days could give it
fn solve_over(tenant: &mut Tenant, hints: &Hints, origin: NaiveDate, ahead: u32) -> (Solved, bool) {
    let until = tenant.today() + Days::new(u64::from(ahead));
    tenant.refresh_calendars(origin, until);
    let tenant = &*tenant;
    let days = (until - origin).num_days() as usize + 1;
    let mut table = Table::from_tenant(tenant, origin, days);
    let net = lower::lower(tenant, &mut table);
    let mut ctx = Context::new(&net, &table);
    let earliest = bound::earliest(&ctx);
    bound::prioritise(&mut ctx, &earliest);
    let found = search::search(&ctx, &earliest, overrides(&net, &table, hints));
    let solved = Solved {
        state: write::write(&ctx, tenant, &earliest, &found.pass),
        hints: self::hints(&net, &table, &found.overrides),
    };
    (solved, found.pass.overflow || earliest.overflow)
}
// }}}

// hints {{{
fn arc_of(net: &Network, item: &Item) -> Option<ArcId> {
    let plan = net.plans.iter().position(|p| p.id == item.0)?;
    net.lowered.get(&(plan as u32, item.1))?.arc
}

fn item_of(net: &Network, arc: ArcId) -> Item {
    let arc = net.arc(arc);
    (net.plans[arc.plan as usize].id, arc.node)
}

/// The hints that still mean something in this network, as overrides
fn overrides(net: &Network, table: &Table, hints: &Hints) -> Overrides {
    let mut overrides = Overrides::default();
    for (first, then) in &hints.after {
        if let (Some(first), Some(then)) = (arc_of(net, first), arc_of(net, then)) {
            overrides.after.entry(first).or_default().push(then);
        }
    }
    for (item, worker, user) in &hints.people {
        let arc = arc_of(net, item);
        let work = arc.and_then(|a| net.arc(a).work());
        let slot = work.and_then(|w| w.slots.iter().position(|s| s.worker == *worker));
        if let (Some(arc), Some(slot), Some(row)) = (arc, slot, table.row(&RowKey::User(*user))) {
            overrides.people.insert((arc, slot), row);
        }
    }
    for (item, days) in &hints.sooner {
        let plan = net.plans.iter().position(|p| p.id == item.0);
        let lowered = plan.and_then(|p| net.lowered.get(&(p as u32, item.1)));
        // the date of work is on its arc, any other is on the vertex the node ends at
        let late = lowered.map(|l| l.arc.map_or(Late::Vertex(l.end), Late::Arc));
        overrides.sooner.extend(late.map(|late| (late, *days)));
    }
    overrides
}

fn hints(net: &Network, table: &Table, overrides: &Overrides) -> Hints {
    let mut hints = Hints::default();
    for (first, others) in &overrides.after {
        let pairs = others
            .iter()
            .map(|then| (item_of(net, *first), item_of(net, *then)));
        hints.after.extend(pairs);
    }
    for ((arc, slot), row) in &overrides.people {
        let worker = net.arc(*arc).work().map(|w| w.slots[*slot].worker);
        if let (Some(worker), RowKey::User(user)) = (worker, table.key(*row)) {
            hints.people.push((item_of(net, *arc), worker, user));
        }
    }
    for (late, days) in &overrides.sooner {
        let item = match late {
            Late::Arc(arc) => Some(item_of(net, *arc)),
            Late::Vertex(v) => {
                let mut ending = net
                    .lowered
                    .iter()
                    .filter(|(_, l)| l.end == *v && l.arc.is_none());
                ending
                    .next()
                    .map(|((plan, node), _)| (net.plans[*plan as usize].id, *node))
            }
        };
        hints.sooner.extend(item.map(|item| (item, *days)));
    }
    hints
        .sooner
        .sort_by_key(|(item, days)| (item.0, item.1.id(), *days));
    hints
        .after
        .sort_by_key(|(a, b)| (a.0, a.1.id(), b.0, b.1.id()));
    hints
        .people
        .sort_by_key(|(item, _, user)| (item.0, item.1.id(), *user));
    hints
}
// }}}
