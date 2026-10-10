// modules {{{
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::{
    planner::graph::Vertex,
    scheduler::{ledger::Ledger, plan_solver::PlanSolver, state::PlanVolatileState},
};
// }}}

/// The most passes of compaction, each can only move things earlier
const MAX_COMPACT_PASSES: usize = 50;

/// What decides who goes next, lowest first: the layer, two measures inside it, the position of
/// the plan, then the node
type Rank = (u8, i64, i64, usize, (u8, Uuid));

/// What is known about a plan between placements, only the plan that just placed changes
struct View {
    /// Every node on the way to a hard date, for the compaction that follows
    on_chain: HashSet<Vertex>,
    /// For each node on the way to a hard date that goes first, the dates it leads to as the day
    /// the node would have to start by if the days of work to the date were all that is in the way
    /// (the date less the work after the node, which is taken off when asked), the earliest first
    /// with the dated node, and how far down the list is not settled yet
    leads: HashMap<Vertex, Leads>,
    /// The next task of the plan to place and how it ranks
    best: Option<(Rank, Vertex)>,
}

struct Leads {
    by: Vec<(i64, Vertex)>,
    /// Everything before this has a dated node that is settled for good
    at: Cell<usize>,
}

impl View {
    fn new(index: usize, solver: &mut PlanSolver, ledger: &Ledger) -> Self {
        let mut on_chain = HashSet::new();
        let mut leads: HashMap<Vertex, Leads> = HashMap::new();
        for (hard, date, chain) in solver.hard_chains() {
            on_chain.extend(chain.iter().copied());
            if !solver.is_urgent(&hard) {
                continue;
            }
            // a node is `remaining(node) - remaining(hard)` days of work before the dated node
            let by = i64::from(date.num_days_from_ce()) - solver.tightening(&hard)
                + solver.remaining(&hard);
            for v in chain {
                leads
                    .entry(v)
                    .or_insert_with(|| Leads {
                        by: Vec::new(),
                        at: Cell::new(0),
                    })
                    .by
                    .push((by, hard));
            }
        }
        for lead in leads.values_mut() {
            lead.by.sort_by_key(|(by, hard)| (*by, hard.key()));
        }
        let mut view = Self {
            on_chain,
            leads,
            best: None,
        };
        view.refresh(index, solver, ledger);
        view
    }

    fn refresh(&mut self, index: usize, solver: &mut PlanSolver, ledger: &Ledger) {
        let ready = solver.advance();
        self.best = ready
            .iter()
            .map(|v| (self.rank(index, solver, solver.ready_slip(v), *v), *v))
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

    /// The latest day, as a day number, the task could start for the nearest date it leads to
    fn latest(&self, solver: &PlanSolver, v: &Vertex) -> Option<i64> {
        let leads = self.leads.get(v)?;
        let mut at = leads.at.get();
        while leads
            .by
            .get(at)
            .is_some_and(|(_, hard)| solver.is_settled(hard))
        {
            at += 1;
        }
        leads.at.set(at);
        leads.by.get(at).map(|(by, _)| by - solver.remaining(v))
    }

    /// Where a ready task goes in the order of placing across plans, in strict layers:
    /// 0. chains to hard dates, the one that must start soonest first
    /// 1. chains to targets, the most behind its plan on its own first, counting priorities
    /// 2. the rest, by plan priority and then how critical it is in its plan
    fn rank(&self, index: usize, solver: &PlanSolver, slip: i64, v: Vertex) -> Rank {
        let key = v.key();
        if let Some(latest) = self.latest(solver, &v) {
            return (0, latest, -solver.remaining(&v), index, key);
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
    for solver in &mut solvers {
        solver.claim_frozen(ledger);
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
    view.on_chain.contains(v) || solver.urgency(v, 0).is_some()
}
