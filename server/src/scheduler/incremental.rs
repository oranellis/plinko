//! Bringing a schedule up to date after updates without working all of it out again: what the
//! updates cannot have changed stays where it was, and only the rest is placed, around it.

use std::{cmp::Reverse, collections::HashMap};

use crate::{
    multitenancy::tenant::Tenant,
    planner::graph::Vertex,
    scheduler::{
        combined,
        impact::{Change, PlanChange},
        ledger::Ledger,
        network::Network,
        plan_solver::PlanSolver,
        state::{Baseline, TenantVolatileState},
    },
    types::{common::TimeConstraint, dependency::PlanNode, short_id::ShortId},
};

/// How bad a schedule is: each hard date missed and the days it is missed by, and each thing
/// left unscheduled
pub fn trouble(schedule: &TenantVolatileState) -> i64 {
    let mut cost = 0;
    for plan in schedule.plans.values() {
        cost += 50 * plan.unscheduled.len() as i64;
        for v in plan.constraint_violations.values() {
            if let TimeConstraint::Fixed(date) | TimeConstraint::Latest(date) = v.constraint {
                cost += 5 + (v.scheduled - date).num_days().max(0);
            }
        }
    }
    cost
}

/// The schedule `previous` with the plans of `change` placed again around the rest, from what
/// `scheduler::prepare` cached. `None` if it cannot be done and the whole tenant should be
/// scheduled. If placing only what depends on the changes leaves more hard dates missed or things
/// unscheduled than before, the plans that changed are placed again as a whole and the better of
/// the two is kept, since what stayed may have been in the way of what had to move.
pub fn reschedule(
    tenant: &Tenant,
    previous: &TenantVolatileState,
    change: &Change,
) -> Option<TenantVolatileState> {
    if change.everything {
        return None;
    }
    let baselines: HashMap<ShortId, _> = tenant
        .plans()
        .keys()
        .map(|id| Some((*id, tenant.baseline(id)?)))
        .collect::<Option<_>>()?;
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
    let ids: Vec<ShortId> = order.into_iter().map(|(_, _, id)| id).collect();
    let partly = place(tenant, previous, change, &ids, &baselines);
    if trouble(&partly) <= trouble(previous) {
        return Some(partly);
    }
    let mut whole = change.clone();
    for plan in whole.plans.values_mut() {
        *plan = PlanChange::Whole;
    }
    let again = place(tenant, previous, &whole, &ids, &baselines);
    Some(if trouble(&again) < trouble(&partly) {
        again
    } else {
        partly
    })
}

fn place(
    tenant: &Tenant,
    previous: &TenantVolatileState,
    change: &Change,
    ids: &[ShortId],
    baselines: &HashMap<ShortId, &Baseline>,
) -> TenantVolatileState {
    let mut ledger = Ledger::from_tenant(tenant, tenant.today());
    let solvers: Vec<PlanSolver> = ids
        .iter()
        .map(|id| {
            let plan = &tenant.plans()[id];
            let mut solver = PlanSolver::new(tenant, plan, Some(baselines[id]));
            let before = previous.plans.get(id);
            if let (Some(before), Some(PlanChange::Nodes(_)) | None) =
                (before, change.plans.get(id))
            {
                // what changed and everything after it is placed again, the rest stays
                let moved = match change.plans.get(id) {
                    Some(PlanChange::Nodes(nodes)) => {
                        let roots = nodes.iter().map(|n| Vertex::Node(*n));
                        Network::new(plan).downstream(roots)
                    }
                    _ => Default::default(),
                };
                let stays = before
                    .tasks
                    .iter()
                    .filter(|(task, _)| plan.tasks().contains_key(task))
                    .filter(|(task, _)| !moved.contains(&Vertex::Node(PlanNode::Task(**task))))
                    .map(|(task, state)| (*task, state.clone()))
                    .collect();
                solver.freeze(stays);
            }
            solver
        })
        .collect();
    let mut states = combined::solve(solvers, &mut ledger);
    for (i, state) in states.iter_mut().enumerate() {
        state.compare_to(&baselines[&ids[i]].state);
    }
    let mut state = TenantVolatileState::from_plans(ids.iter().copied().zip(states).collect());
    for (user, schedule) in &mut state.users {
        schedule.bottleneck = ledger.load(user);
    }
    state
}
