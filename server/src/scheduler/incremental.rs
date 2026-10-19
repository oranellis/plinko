//! Bringing a schedule up to date after updates without working all of it out again: what the
//! updates cannot have changed stays where it was, and only the rest is placed, around it.

// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::{
    multitenancy::tenant::Tenant,
    planner::graph::Vertex,
    scheduler::{
        Fixes, combined,
        impact::{Change, PlanChange},
        ledger::Ledger,
        network::Network,
        plan_order,
        plan_solver::PlanSolver,
        state::TenantVolatileState,
    },
    types::{common::TimeConstraint, dependency::PlanNode, short_id::ShortId},
};
// }}}

// fn trouble {{{
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
// }}}

// fn reschedule {{{
/// The schedule `previous` with the plans of `change` placed again around the rest, from what
/// `scheduler::prepare` cached. `None` if it cannot be done and the whole tenant should be
/// scheduled. If placing only what depends on the changes leaves more hard dates missed or things
/// unscheduled than before, the plans that changed are placed again as a whole and the better of
/// the two is kept, since what stayed may have been in the way of what had to move.
pub fn reschedule(
    tenant: &Tenant,
    previous: &TenantVolatileState,
    change: &Change,
    fixes: &Fixes,
) -> Option<TenantVolatileState> {
    if change.everything {
        return None;
    }
    let baselines: HashMap<ShortId, _> = tenant
        .plans()
        .keys()
        .map(|id| Some((*id, tenant.baseline(id)?)))
        .collect::<Option<_>>()?;
    let ids = plan_order(tenant, |id| baselines[id].state.end());
    let moved: HashMap<ShortId, HashSet<Vertex>> = change
        .plans
        .iter()
        .filter_map(|(id, plan_change)| match plan_change {
            PlanChange::Nodes(nodes) => {
                let roots = nodes.iter().map(|n| Vertex::Node(*n));
                Some((*id, Network::new(tenant.plans().get(id)?).downstream(roots)))
            }
            PlanChange::Whole => None,
        })
        .collect();
    // the first day anything that moves was on, from which later work may have to give way
    let first = moved
        .iter()
        .flat_map(|(id, set)| set.iter().map(move |v| (id, v)))
        .filter_map(|(id, v)| match v {
            Vertex::Node(PlanNode::Task(task)) => previous.plans.get(id)?.tasks.get(task),
            _ => None,
        })
        .map(|t| t.date_range.start())
        .min();
    let place = |from: Option<NaiveDate>| {
        let mut ledger = Ledger::from_tenant(tenant, tenant.today());
        let solvers: Vec<PlanSolver> = ids
            .iter()
            .map(|id| {
                let mut solver = PlanSolver::new(tenant, &tenant.plans()[id], Some(baselines[id]));
                solver.repair(fixes.get(id).cloned().unwrap_or_default());
                let stays = match (previous.plans.get(id), change.plans.get(id)) {
                    (Some(before), Some(PlanChange::Nodes(_)) | None) => before
                        .tasks
                        .iter()
                        .filter(|(task, _)| tenant.plans()[id].tasks().contains_key(task))
                        .filter(|(task, _)| {
                            let task = Vertex::Node(PlanNode::Task(**task));
                            !moved.get(id).is_some_and(|set| set.contains(&task))
                        })
                        .filter(|(_, t)| from.is_none_or(|day| t.date_range.start() < day))
                        .map(|(task, state)| (*task, state.clone()))
                        .collect(),
                    _ => HashMap::new(),
                };
                solver.freeze(stays);
                solver
            })
            .collect();
        let mut states = combined::solve(solvers, &mut ledger, true);
        for (i, state) in states.iter_mut().enumerate() {
            state.compare_to(&baselines[&ids[i]].state);
        }
        let mut state = TenantVolatileState::from_plans(ids.iter().copied().zip(states).collect());
        for (user, schedule) in &mut state.users {
            schedule.bottleneck = ledger.load(user);
        }
        state
    };
    // what depends on the changes is placed again, and if that is not enough everything after the
    // first of them is, as work that stayed may have been in the way of what had to move
    let dependents = place(None);
    if trouble(&dependents) <= trouble(previous) || first.is_none() {
        return Some(dependents);
    }
    let later = place(first);
    Some(if trouble(&later) < trouble(&dependents) {
        later
    } else {
        dependents
    })
}
// }}}
