// modules {{{
use crate::scheduler::{
    arc::{ArcId, ArcKind},
    day::Day,
    network::VertexId,
    pass::{Context, Pass, Rules},
};
// }}}

// earliest {{{
/// Everything as early as it could be if nobody ever ran out of hours: each person's calendar
/// and each task's affinities still count, other work does not. What goals are measured against.
pub fn earliest(ctx: &Context) -> Pass {
    let rules = Rules {
        unlimited: true,
        ..Rules::default()
    };
    let mut pass = Pass::new(ctx, &rules);
    pass.run(ctx, &rules, None);
    pass
}
// }}}

// prioritise {{{
/// Fills in how late each arc may start and how much room it has, from the earliest schedule.
/// Hard dates are taken as they are. A target is due when it could be reached at the earliest,
/// and so is the end of each plan, each sooner by its priority in days.
pub fn prioritise(ctx: &mut Context, earliest: &Pass) {
    let net = ctx.net;
    let none = || vec![i64::MAX; net.vertices.len()];
    let due = |v: VertexId, priority: i64| {
        let at = earliest.time[v.0 as usize]?;
        Some((v.0 as usize, i64::from(at.0) - priority))
    };
    let mut hard = none();
    for (i, vertex) in net.vertices.iter().enumerate() {
        hard[i] = vertex.deadline.map_or(i64::MAX, |d| i64::from(d.0));
    }
    let (mut targets, mut ends) = (none(), none());
    for plan in &net.plans {
        for target in &plan.targets {
            if let Some((i, day)) = due(target.vertex, plan.priority + target.priority) {
                targets[i] = targets[i].min(day);
            }
        }
        if let Some((i, day)) = due(plan.end, plan.priority) {
            ends[i] = day;
        }
    }
    let hard = latest(ctx, earliest, hard, true);
    let targets = latest(ctx, earliest, targets, false);
    let ends = latest(ctx, earliest, ends, false);
    ctx.latest = (0..net.arcs.len())
        .map(|i| [hard[i], targets[i], ends[i]])
        .collect();
    ctx.room = net
        .arc_ids()
        .map(|id| {
            let placed = earliest.placed[id.0 as usize].as_ref();
            let work = net.arc(id).work().zip(placed);
            work.map_or(0, |(work, placed)| {
                i64::from(ctx.valid(id, work, &placed.rows).1.count())
            })
        })
        .collect();
}
// }}}

// latest {{{
/// The latest each arc may start so that no vertex passes the day `limit` gives it, working
/// back with the spans of the earliest schedule. `dated` adds the hard dates arcs carry.
fn latest(ctx: &Context, earliest: &Pass, mut limit: Vec<i64>, dated: bool) -> Vec<i64> {
    let net = ctx.net;
    let mut starts = vec![i64::MAX; net.arcs.len()];
    for v in net.order.iter().rev() {
        let vertex = net.vertex(*v);
        let i = v.0 as usize;
        let next = vertex.next.iter().map(|w| limit[w.0 as usize]);
        // work that has started does not wait for what it depends on any more
        let open = |a: &&ArcId| net.arc(**a).work().is_none_or(|w| w.is_open());
        let outgoing = vertex.outgoing.iter().filter(open);
        let outgoing = outgoing.map(|a| starts[a.0 as usize]);
        limit[i] = next.chain(outgoing).fold(limit[i], i64::min);
        for arc in &vertex.incoming {
            starts[arc.0 as usize] = start_by(ctx, earliest, *arc, limit[i], dated);
        }
    }
    starts
}

/// The latest an arc may start to end by `end`
fn start_by(ctx: &Context, earliest: &Pass, id: ArcId, end: i64, dated: bool) -> i64 {
    let Some(placed) = &earliest.placed[id.0 as usize] else {
        return i64::MAX;
    };
    let arc = ctx.net.arc(id);
    let length = i64::from(placed.end.0) - i64::from(placed.start.0);
    let work = match &arc.kind {
        ArcKind::Buffer(_) if end == i64::MAX => return end,
        ArcKind::Buffer(_) => return end - length,
        ArcKind::Work(work) => work,
    };
    let own = |day: Option<Day>| day.filter(|_| dated).map_or(i64::MAX, |d| i64::from(d.0));
    let end = end.min(own(work.latest_end));
    if end == i64::MAX {
        return own(work.latest_start);
    }
    let (_, valid) = ctx.valid(id, work, &placed.rows);
    let by = Day(end.clamp(0, ctx.table.days as i64) as u32);
    let back = match work.days {
        0 => Some(by),
        days => valid.nth_back(by, days),
    };
    let start = match back {
        Some(start) => i64::from(start.0) + end.min(0),
        // not enough days before it, so it is as far behind as the days it lacks
        None => end.min(0) - i64::from(work.days) + i64::from(valid.rank(by)),
    };
    start.min(own(work.latest_start))
}
// }}}
