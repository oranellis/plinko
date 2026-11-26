// modules {{{
use crate::{
    scheduler::state::UnscheduledReason,
    scheduler::{
        arc::ArcId,
        day::Day,
        network::VertexId,
        pass::{Context, Pass},
    },
};
// }}}

/// How good a schedule is, lower is better and an earlier field always outweighs a later one
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Objective {
    /// Work that found no days at all
    pub unplaced: i64,
    /// Days hard dates are missed by in all, and how many are missed
    pub missed_days: i64,
    pub missed: i64,
    /// The latest goal and all of them together, in days after the earliest each could be,
    /// counting priorities as days
    pub worst: i64,
    pub total: i64,
    /// The end days of all work added up, so that loose work is early too
    pub ends: i64,
}

// Objective supporting types {{{
/// Something a schedule is judged on that is not where it should be
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Late {
    Vertex(VertexId),
    Arc(ArcId),
}

/// A schedule's score and what is behind the first part of it that is not zero
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    pub objective: Objective,
    pub late: Vec<Late>,
}
// }}}

// score {{{
/// Scores a finished pass against the earliest schedule
pub fn score(ctx: &Context, earliest: &Pass, pass: &Pass) -> Score {
    let net = ctx.net;
    let mut objective = Objective::default();
    let (mut missed, mut goals): (Vec<Late>, Vec<(i64, Late)>) = (Vec::new(), Vec::new());
    let over = |at: Day, limit: Option<Day>| {
        limit.map_or(0, |l| (i64::from(at.0) - i64::from(l.0)).max(0))
    };
    for id in net.arc_ids() {
        let i = id.0 as usize;
        let no_room = pass.failed[i] == Some(UnscheduledReason::NoCapacity);
        objective.unplaced += i64::from(no_room);
        let (Some(work), Some(placed)) = (net.arc(id).work(), &pass.placed[i]) else {
            continue;
        };
        if !work.is_open() {
            continue;
        }
        objective.ends += i64::from(placed.end.0);
        for days in [
            over(placed.start, work.latest_start),
            over(placed.end, work.latest_end),
        ] {
            if days > 0 {
                objective.missed_days += days;
                objective.missed += 1;
                missed.push(Late::Arc(id));
            }
        }
    }
    for (i, vertex) in net.vertices.iter().enumerate() {
        let days = pass.time[i].map_or(0, |at| over(at, vertex.deadline));
        if days > 0 {
            objective.missed_days += days;
            objective.missed += 1;
            missed.push(Late::Vertex(VertexId(i as u32)));
        }
    }
    for plan in &net.plans {
        let targets = plan.targets.iter().map(|t| (t.vertex, t.priority));
        let mut aims: Vec<(VertexId, i64)> = targets.collect();
        if aims.is_empty() {
            aims.push((plan.end, 0));
        }
        for (vertex, priority) in aims {
            let i = vertex.0 as usize;
            let (Some(at), Some(soonest)) = (pass.time[i], earliest.time[i]) else {
                continue;
            };
            let late = (i64::from(at.0) - i64::from(soonest.0)).max(0);
            let counted = late + plan.priority + priority;
            objective.total += counted;
            objective.worst = objective.worst.max(counted);
            if late > 0 {
                goals.push((counted, Late::Vertex(vertex)));
            }
        }
    }
    let late = match missed.is_empty() {
        false => missed,
        true => {
            // the goals furthest behind of those that are behind at all
            let worst = goals.iter().map(|(counted, _)| *counted).max();
            let behind = goals.iter().filter(|(c, _)| Some(*c) == worst);
            behind.map(|(_, late)| *late).collect()
        }
    };
    Score { objective, late }
}
// }}}
