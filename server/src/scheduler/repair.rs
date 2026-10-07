// modules {{{
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use uuid::Uuid;

use crate::{
    multitenancy::tenant::Tenant,
    scheduler::{
        combined,
        ledger::Ledger,
        plan_solver::{PlanSolver, eligible},
        state::{Baseline, PlanVolatileState},
    },
    types::{
        dependency::PlanNode,
        short_id::ShortId,
        task::{Assignee, Status, Worker},
    },
};
// }}}

/// The most rounds of repair, each takes the best change that improves the result
const MAX_ROUNDS: usize = 8;

/// A person booked this much of their time is a bottleneck for work that waits on them
const BOTTLENECK_LOAD: f32 = 0.9;

/// What rounds have changed, for each plan in the order they are scheduled in
#[derive(Clone)]
struct Repairs {
    bumps: Vec<HashMap<Option<Uuid>, i64>>,
    forced: Vec<HashMap<(Uuid, Worker), Uuid>>,
}

/// A target and how far behind it is
struct Reached {
    plan: usize,
    target: Option<Uuid>,
    /// Days later than the plan on its own
    delay: i64,
    /// The delay counting plan and target priority as days
    effective: i64,
}

/// A schedule of every plan with how well it did
pub struct Outcome {
    pub states: Vec<PlanVolatileState>,
    pub ledger: Ledger,
    reached: Vec<Reached>,
}

impl Outcome {
    /// The worst effective delay, then the total, lower is better
    fn objective(&self) -> (i64, i64) {
        let worst = self.reached.iter().map(|r| r.effective).max();
        (
            worst.unwrap_or(0),
            self.reached.iter().map(|r| r.effective).sum(),
        )
    }

    /// The targets furthest behind, if any is behind at all
    fn worst(&self) -> Vec<&Reached> {
        let worst = self.reached.iter().map(|r| r.effective).max();
        let behind = self.reached.iter().filter(|r| r.delay > 0);
        behind.filter(|r| Some(r.effective) == worst).collect()
    }
}

fn run(
    tenant: &Tenant,
    ids: &[ShortId],
    baselines: &HashMap<ShortId, Cow<Baseline>>,
    repairs: &Repairs,
) -> Outcome {
    let mut ledger = Ledger::from_tenant(tenant, tenant.today());
    let solvers = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut solver = PlanSolver::new(tenant, &tenant.plans()[id], Some(&*baselines[id]));
            solver.repair(repairs.bumps[i].clone(), repairs.forced[i].clone());
            solver
        })
        .collect();
    let mut states = combined::solve(solvers, &mut ledger);
    let mut reached = Vec::new();
    for (i, state) in states.iter_mut().enumerate() {
        state.compare_to(&baselines[&ids[i]].state);
        let plan = &tenant.plans()[&ids[i]];
        let priority = i64::from(plan.scheduler_data().priority());
        let mut goals: Vec<_> = plan
            .targets()
            .iter()
            .filter_map(|(id, t)| Some((Some(*id), *state.targets.get(id)?, t.priority())))
            .collect();
        if goals.is_empty() {
            goals.extend(state.plan_end.map(|end| (None, end, 0)));
        }
        goals.sort_by_key(|(id, _, _)| *id);
        for (target, forecast, target_priority) in goals {
            let delay = forecast.delay();
            let effective = delay + priority + i64::from(target_priority);
            reached.push(Reached {
                plan: i,
                target,
                delay,
                effective,
            });
        }
    }
    Outcome {
        states,
        ledger,
        reached,
    }
}

/// Makes each worst target more urgent by as many days as it is late
fn bump(worst: &[&Reached], repairs: &Repairs) -> Repairs {
    let mut next = repairs.clone();
    for r in worst {
        *next.bumps[r.plan].entry(r.target).or_insert(0) += r.delay.max(1);
    }
    next
}

/// Moves tag and group work off the bottleneck people the worst targets wait on, to whoever
/// eligible is least booked, `None` if there is nothing to move
fn reassign(
    tenant: &Tenant,
    ids: &[ShortId],
    baselines: &HashMap<ShortId, Cow<Baseline>>,
    outcome: &Outcome,
    worst: &[&Reached],
    repairs: &Repairs,
) -> Option<Repairs> {
    let load = |user: &Uuid| outcome.ledger.load(user);
    let mut hot: HashSet<Uuid> = HashSet::new();
    for r in worst {
        let by_target = &baselines[&ids[r.plan]].by_target;
        let Some(chain) = by_target.iter().find(|t| t.id == r.target) else {
            continue;
        };
        for node in chain.slack.keys() {
            let PlanNode::Task(task) = node else {
                continue;
            };
            let assigned = outcome.states[r.plan]
                .tasks
                .get(task)
                .map(|t| &t.assignments);
            let users = assigned
                .into_iter()
                .flatten()
                .filter_map(|(who, _)| match who {
                    Assignee::User(user) => Some(*user),
                    Assignee::Placeholder => None,
                });
            hot.extend(users.filter(|u| load(u) >= BOTTLENECK_LOAD));
        }
    }
    let mut next = repairs.clone();
    let mut changed = false;
    for (i, id) in ids.iter().enumerate() {
        let plan = &tenant.plans()[id];
        let mut tasks: Vec<_> = outcome.states[i].tasks.iter().collect();
        tasks.sort_by_key(|(task, _)| **task);
        for (task, state) in tasks {
            let asked = &plan.tasks()[task];
            if asked.persistent_state().status != Status::NotStarted {
                continue;
            }
            for (worker, who) in &state.workers {
                let Assignee::User(user) = who else {
                    continue;
                };
                let movable = matches!(worker, Worker::Tag(_) | Worker::UserGroup(_));
                if !movable || !hot.contains(user) || asked.actual_workers().contains_key(worker) {
                    continue;
                }
                let others = eligible(tenant, plan, worker).into_iter();
                let spare = others
                    .filter(|o| o != user && load(o) < load(user))
                    .min_by(|a, b| load(a).total_cmp(&load(b)));
                if let Some(spare) = spare {
                    next.forced[i].insert((*task, *worker), spare);
                    changed = true;
                }
            }
        }
    }
    changed.then_some(next)
}

/// Schedules the plans, in the order given, then repairs the result in rounds. A round tries
/// making the worst targets more urgent and moving work off the bottleneck people they wait on,
/// each by rescheduling everything, and keeps the better one if it beats the worst effective delay
/// and then the total. Stops when nothing does or after `MAX_ROUNDS`.
pub fn solve(
    tenant: &Tenant,
    ids: &[ShortId],
    baselines: &HashMap<ShortId, Cow<Baseline>>,
) -> Outcome {
    let mut repairs = Repairs {
        bumps: vec![HashMap::new(); ids.len()],
        forced: vec![HashMap::new(); ids.len()],
    };
    let mut best = run(tenant, ids, baselines, &repairs);
    for _ in 0..MAX_ROUNDS {
        let worst = best.worst();
        let moves = [
            Some(bump(&worst, &repairs)),
            reassign(tenant, ids, baselines, &best, &worst, &repairs),
        ];
        // the changes are independent, so each is scheduled on its own thread
        let tried: Vec<(Outcome, Repairs)> = std::thread::scope(|scope| {
            let running: Vec<_> = moves
                .into_iter()
                .flatten()
                .map(|next| {
                    scope.spawn(|| {
                        let outcome = run(tenant, ids, baselines, &next);
                        (outcome, next)
                    })
                })
                .collect();
            running
                .into_iter()
                .map(|r| r.join().expect("a schedule does not panic"))
                .collect()
        });
        let better = tried
            .into_iter()
            .filter(|(outcome, _)| outcome.objective() < best.objective())
            .min_by_key(|(outcome, _)| outcome.objective());
        let Some((outcome, next)) = better else {
            break;
        };
        (best, repairs) = (outcome, next);
    }
    best
}
