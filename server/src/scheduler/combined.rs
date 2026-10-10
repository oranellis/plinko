// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::{
    planner::graph::Vertex,
    scheduler::{ledger::Ledger, plan_solver::PlanSolver, state::PlanVolatileState},
};
// }}}

/// The most passes of compaction, each can only move things earlier
const MAX_COMPACT_PASSES: usize = 50;

/// Days of slack at or below which a chain to a hard date goes before everything else
const HARD_MARGIN_DAYS: i64 = 3;

/// What decides who goes next, lowest first: the layer, two measures inside it, the position of
/// the plan, then the node
type Rank = (u8, i64, i64, usize, (u8, Uuid));

/// What is known about a plan between placements, only the plan that just placed changes
struct View {
    chains: Vec<(Vertex, NaiveDate, HashSet<Vertex>)>,
    /// The next task of the plan to place and how it ranks
    best: Option<(Rank, Vertex)>,
}

impl View {
    fn new(index: usize, solver: &mut PlanSolver, ledger: &Ledger) -> Self {
        let mut view = Self {
            chains: solver.hard_chains(),
            best: None,
        };
        view.refresh(index, solver, ledger);
        view
    }

    fn refresh(&mut self, index: usize, solver: &mut PlanSolver, ledger: &Ledger) {
        let ready = solver.advance();
        // working out the slip of everything is only needed to see how close hard dates are
        let slips = if self.chains.is_empty() {
            HashMap::new()
        } else {
            solver.slips()
        };
        let slip = |v: &Vertex| {
            slips
                .get(v)
                .copied()
                .unwrap_or_else(|| solver.ready_slip(v))
        };
        self.best = ready
            .iter()
            .map(|v| (self.rank(index, solver, &slips, slip(v), *v), *v))
            .min_by_key(|(rank, _)| *rank);
        // a task with a date it must start by goes ahead of one that would run past that date,
        // whatever its own measures say, since only the order can save it
        if let Some((_, next)) = self.best
            && let Some((v, date, over)) = solver.pressed(&ready, &next, ledger)
        {
            let rank = (0, -over, i64::from(date.num_days_from_ce()), index, v.key());
            self.best = Some((rank, v));
        }
    }

    /// Where a ready task goes in the order of placing across plans, in strict layers:
    /// 0. chains to a hard date that is close, the closest first
    /// 1. chains to targets, the most behind its plan on its own first, counting priorities
    /// 2. the rest, by plan priority and then how critical it is in its plan
    fn rank(
        &self,
        index: usize,
        solver: &PlanSolver,
        slips: &HashMap<Vertex, i64>,
        slip: i64,
        v: Vertex,
    ) -> Rank {
        let key = v.key();
        let close = self
            .chains
            .iter()
            .filter(|(_, _, chain)| chain.contains(&v))
            .filter_map(|(hard, date, _)| Some((solver.hard_slack(hard, *date, slips)?, *date)))
            .filter(|(slack, _)| *slack <= HARD_MARGIN_DAYS)
            .min();
        if let Some((slack, date)) = close {
            return (0, slack, i64::from(date.num_days_from_ce()), index, key);
        }
        if let Some((urgency, leads)) = solver.urgency(&v, slip) {
            return (1, -urgency, -(leads as i64), index, key);
        }
        let (priority, slack) = solver.rest_rank(&v);
        (2, priority, slack, index, key)
    }
}

/// Schedules the plans together against one ledger, in the order they are given, which settles
/// ties. Finished and held work goes in first, then work in progress, then everything else one
/// task at a time, whichever is next by `rank` among those ready, so each plan's delay counts
/// against what it lost to others. Returns the plans in the order given.
pub fn solve(mut solvers: Vec<PlanSolver>, ledger: &mut Ledger) -> Vec<PlanVolatileState> {
    for in_progress in [false, true] {
        for solver in &mut solvers {
            solver.claim_history(ledger, in_progress);
        }
    }
    let mut views: Vec<View> = solvers
        .iter_mut()
        .enumerate()
        .map(|(i, solver)| View::new(i, solver, ledger))
        .collect();
    while let Some((i, v)) = views
        .iter()
        .enumerate()
        .filter_map(|(i, view)| Some((view.best?, i)))
        .min_by_key(|((rank, _), _)| *rank)
        .map(|((_, v), i)| (i, v))
    {
        solvers[i].place_open(v, ledger);
        views[i].refresh(i, &mut solvers[i], ledger);
    }
    for solver in &mut solvers {
        solver.sweep_stuck();
    }
    compact(&mut solvers, &views, ledger);
    solvers.into_iter().map(PlanSolver::finish).collect()
}

/// Pulls tasks of every plan into room left by others. Chains to hard dates and targets go first,
/// earliest start first, so loose work does not take room they can still use, and the rest follow
/// once they have all had their chance. Repeats until nothing ends sooner.
fn compact(solvers: &mut [PlanSolver], views: &[View], ledger: &mut Ledger) {
    let mut layers: [Vec<(NaiveDate, usize, Vertex)>; 2] = Default::default();
    for _ in 0..MAX_COMPACT_PASSES {
        for layer in &mut layers {
            layer.clear();
        }
        for (i, solver) in solvers.iter().enumerate() {
            for (start, v) in solver.compactable() {
                layers[usize::from(!on_chain(solver, &views[i], &v))].push((start, i, v));
            }
        }
        let mut moved = false;
        for layer in &mut layers {
            layer.sort_by_key(|(start, i, v)| (*start, *i, v.key()));
            for (_, i, v) in layer.iter() {
                moved |= solvers[*i].try_compact(*v, ledger);
            }
            if moved {
                break;
            }
        }
        if !moved {
            break;
        }
    }
}

/// Whether a node leads to a hard date or a target
fn on_chain(solver: &PlanSolver, view: &View, v: &Vertex) -> bool {
    view.chains.iter().any(|(_, _, chain)| chain.contains(v)) || solver.urgency(v, 0).is_some()
}
