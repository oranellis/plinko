// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::{
    planner::graph::Vertex,
    scheduler::{ledger::Ledger, plan_solver::PlanSolver, state::PlanVolatileState},
};
// }}}

/// Days of slack at or below which a chain to a hard date goes before everything else
const HARD_MARGIN_DAYS: i64 = 3;

/// What decides who goes next, lowest first: the layer, two measures inside it, the position of
/// the plan, then the node
type Rank = (u8, i64, i64, usize, (u8, Uuid));

/// What is known about a plan between placements, only the plan that just placed changes
struct View {
    slips: HashMap<Vertex, i64>,
    chains: Vec<(Vertex, NaiveDate, HashSet<Vertex>)>,
    ready: Vec<Vertex>,
}

impl View {
    fn new(solver: &mut PlanSolver) -> Self {
        let mut view = Self {
            slips: HashMap::new(),
            chains: solver.hard_chains(),
            ready: Vec::new(),
        };
        view.refresh(solver);
        view
    }

    fn refresh(&mut self, solver: &mut PlanSolver) {
        self.ready = solver.advance();
        self.slips = solver.slips();
    }
}

/// Where a ready task goes in the order of placing across plans, in strict layers:
/// 0. chains to a hard date that is close, the closest first
/// 1. chains to targets, the most behind its plan on its own first, counting priorities
/// 2. the rest, by plan priority and then how critical it is in its plan
fn rank(index: usize, solver: &PlanSolver, view: &View, v: Vertex) -> Rank {
    let key = v.key();
    let close = view
        .chains
        .iter()
        .filter(|(_, _, chain)| chain.contains(&v))
        .filter_map(|(hard, date, _)| Some((solver.hard_slack(hard, *date, &view.slips)?, *date)))
        .filter(|(slack, _)| *slack <= HARD_MARGIN_DAYS)
        .min();
    if let Some((slack, date)) = close {
        return (0, slack, i64::from(date.num_days_from_ce()), index, key);
    }
    if let Some((urgency, leads)) = solver.urgency(&v, &view.slips) {
        return (1, -urgency, -(leads as i64), index, key);
    }
    let (priority, slack) = solver.rest_rank(&v);
    (2, priority, slack, index, key)
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
    let mut views: Vec<View> = solvers.iter_mut().map(View::new).collect();
    loop {
        let ranked = views.iter().enumerate().flat_map(|(i, view)| {
            let solver = &solvers[i];
            view.ready
                .iter()
                .map(move |v| (rank(i, solver, view, *v), i, *v))
        });
        let Some((_, i, v)) = ranked.min_by_key(|(rank, _, _)| *rank) else {
            break;
        };
        solvers[i].place_open(v, ledger);
        views[i].refresh(&mut solvers[i]);
    }
    for solver in &mut solvers {
        solver.sweep_stuck();
        solver.compact(ledger);
    }
    solvers.into_iter().map(PlanSolver::finish).collect()
}
