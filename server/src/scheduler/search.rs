// modules {{{
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use crate::scheduler::{
    arc::ArcId,
    justify::justify,
    network::VertexId,
    objective::{Late, Score, score},
    pass::{Context, Overrides, Pass, Rules},
    table::RowId,
};
// }}}

// constants {{{
/// Placements a search may make for each arc that takes people's hours
const BUDGET: usize = 30;

/// A search stops after this long whatever is left of its budget, which is the only way two
/// runs over the same tenant can differ
const TIME_LIMIT: Duration = Duration::from_secs(1);

/// How many flips are tried at once, the best of which is kept
const TRIED_TOGETHER: usize = 3;

/// How many flips are looked at from one schedule
const MOST_FLIPS: usize = 12;
// }}}

/// A choice to make the other way
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flip {
    /// `first` took hours `then` was waiting for, so it waits until `then` is placed
    After { first: ArcId, then: ArcId },
    /// A slot goes to another of the people who could take it
    Person { arc: ArcId, slot: usize, row: RowId },
}

// Flip supporting types {{{
/// A schedule with the overrides that gave it
pub struct Found {
    pub pass: Pass,
    pub score: Score,
    pub overrides: Overrides,
}

/// One schedule tried: the first pass with its snapshots, and the better of it and what
/// justifying it gives
struct Trial {
    overrides: Overrides,
    first: Pass,
    snapshots: Vec<(usize, Pass)>,
    best: Pass,
    score: Score,
    /// Placements it took
    cost: usize,
}
// }}}

// search {{{
/// The best schedule found by placing everything, then making contested choices the other way
/// where that is better, starting from the overrides of the last search if they still help
pub fn search(ctx: &Context, earliest: &Pass, hints: Overrides) -> Found {
    let stop = Instant::now() + TIME_LIMIT;
    let contended = ctx.net.arc_ids().filter(|a| ctx.contended(*a)).count();
    let budget = BUDGET * contended;
    let plain = trial(ctx, earliest, Overrides::default(), None);
    let mut spent = plain.cost;
    let mut base = plain;
    if hints != Overrides::default() {
        let hinted = trial(ctx, earliest, hints, None);
        spent += hinted.cost;
        if hinted.score.objective < base.score.objective {
            base = hinted;
        }
    }
    let mut tried: HashSet<Flip> = HashSet::new();
    while spent < budget && Instant::now() < stop {
        let flips: Vec<Flip> = flips(ctx, &base)
            .into_iter()
            .filter(|flip| !tried.contains(flip))
            .take(TRIED_TOGETHER)
            .collect();
        if flips.is_empty() {
            break;
        }
        tried.extend(flips.iter().copied());
        let base_ref = &base;
        let trials: Vec<Trial> = std::thread::scope(|scope| {
            let running: Vec<_> = flips
                .iter()
                .map(|flip| {
                    scope.spawn(move || {
                        let overrides = with(base_ref.overrides.clone(), *flip);
                        trial(ctx, earliest, overrides, Some((base_ref, *flip)))
                    })
                })
                .collect();
            let joined = running.into_iter();
            joined
                .map(|r| r.join().expect("a schedule does not panic"))
                .collect()
        });
        spent += trials.iter().map(|t| t.cost).sum::<usize>();
        let better = trials.into_iter().min_by_key(|t| t.score.objective);
        if let Some(better) = better.filter(|t| t.score.objective < base.score.objective) {
            base = better;
        }
    }
    Found {
        pass: base.best,
        score: base.score,
        overrides: base.overrides,
    }
}
// }}}

// trial {{{
/// Schedules with the overrides. A trial that differs from `from` by one flip starts from the
/// last snapshot of it that the flip cannot have changed.
fn trial(
    ctx: &Context,
    earliest: &Pass,
    overrides: Overrides,
    from: Option<(&Trial, Flip)>,
) -> Trial {
    let rules = Rules {
        overrides: Some(&overrides),
        ..Rules::default()
    };
    let resumed = from.and_then(|(base, flip)| {
        let moved = match flip {
            Flip::After { first, .. } => first,
            Flip::Person { arc, .. } => arc,
        };
        let step = base.first.steps.iter().position(|a| *a == moved)?;
        let kept = base.snapshots.iter().take_while(|(at, _)| *at <= step);
        let kept: Vec<(usize, Pass)> = kept.cloned().collect();
        Some((kept.last()?.1.clone(), kept))
    });
    let (mut first, mut snapshots) = match resumed {
        Some(resumed) => resumed,
        None => (Pass::new(ctx, &rules), Vec::new()),
    };
    let before = first.steps.len();
    first.run(ctx, &rules, Some(&mut snapshots));
    let justified = justify(ctx, &first);
    let cost = first.steps.len() - before + 2 * justified.steps.len();
    let (plain, pushed) = (
        score(ctx, earliest, &first),
        score(ctx, earliest, &justified),
    );
    let (best, score) = match pushed.objective < plain.objective {
        true => (justified, pushed),
        false => (first.clone(), plain),
    };
    Trial {
        overrides,
        first,
        snapshots,
        best,
        score,
        cost,
    }
}

fn with(mut overrides: Overrides, flip: Flip) -> Overrides {
    match flip {
        Flip::After { first, then } => overrides.after.entry(first).or_default().push(then),
        Flip::Person { arc, slot, row } => drop(overrides.people.insert((arc, slot), row)),
    }
    overrides
}
// }}}

// flips {{{
/// The flips worth trying on a schedule, the most promising first. They are found by walking
/// back from what is late over whatever decided each start: the vertex an arc waited for, or
/// the arc that held the hours it wanted.
fn flips(ctx: &Context, trial: &Trial) -> Vec<Flip> {
    let (net, pass) = (ctx.net, &trial.best);
    let mut found: Vec<(i64, Flip)> = Vec::new();
    let mut seen: HashSet<Late> = HashSet::new();
    let mut stack: Vec<Late> = trial.score.late.clone();
    let day = |d: crate::scheduler::day::Day| i64::from(d.0);
    while let Some(late) = stack.pop() {
        if !seen.insert(late) || found.len() >= MOST_FLIPS {
            continue;
        }
        match late {
            Late::Vertex(v) => {
                let vertex = net.vertex(v);
                let at = pass.time[v.0 as usize];
                let held = |u: &&VertexId| pass.time[u.0 as usize] == at;
                stack.extend(vertex.after.iter().filter(held).map(|u| Late::Vertex(*u)));
                let ends = |a: &&ArcId| pass.placed[a.0 as usize].as_ref().map(|p| p.end) == at;
                stack.extend(vertex.incoming.iter().filter(ends).map(|a| Late::Arc(*a)));
            }
            Late::Arc(id) => {
                let (arc, Some(placed)) = (net.arc(id), &pass.placed[id.0 as usize]) else {
                    continue;
                };
                if arc.work().is_some_and(|w| !w.is_open()) {
                    continue;
                }
                let waited = day(placed.start) - day(placed.ready);
                if let Some((_, first)) = placed.blocker.filter(|(_, first)| *first != id) {
                    let reversed =
                        |o: &Overrides| o.after.get(&id).is_some_and(|a| a.contains(&first));
                    let holder = pass.placed[first.0 as usize].as_ref();
                    if let Some(holder) = holder.filter(|_| !reversed(&trial.overrides)) {
                        // what the one in the way has to spare, against what it would lose
                        let latest = ctx.latest[first.0 as usize].iter().min();
                        let spare = latest.map_or(0, |l| l.saturating_sub(day(holder.start)));
                        let lost = (day(placed.end) - day(placed.start) - spare.max(0)).max(0);
                        found.push((waited - lost, Flip::After { first, then: id }));
                    }
                    stack.push(Late::Arc(first));
                }
                if let Some((slot, row)) = placed.other {
                    found.push((waited, Flip::Person { arc: id, slot, row }));
                }
                if pass.time[arc.start.0 as usize] == Some(placed.ready) {
                    stack.push(Late::Vertex(arc.start));
                }
            }
        }
    }
    found.sort_by_key(|(gain, flip)| (std::cmp::Reverse(*gain), flip_key(flip)));
    found.dedup_by_key(|(_, flip)| *flip);
    found.into_iter().map(|(_, flip)| flip).collect()
}

/// A fixed order of flips, for ones that promise the same
fn flip_key(flip: &Flip) -> (u8, u32, u32, u32) {
    match flip {
        Flip::After { first, then } => (0, first.0, then.0, 0),
        Flip::Person { arc, slot, row } => (1, arc.0, *slot as u32, row.0),
    }
}
// }}}
