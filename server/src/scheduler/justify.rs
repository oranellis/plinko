// modules {{{
use std::collections::BinaryHeap;

use crate::scheduler::{
    arc::{ArcId, ArcKind, Until},
    day::Day,
    network::VertexId,
    pass::{Context, Pass, Rules},
};
// }}}

// justify {{{
/// A schedule from one that is finished: everything is pushed as late as it can go without any
/// goal ending later, then placed again as early as it can go in the order that left. Work that
/// went early only because what mattered more was not ready yet ends up behind it.
pub fn justify(ctx: &Context, pass: &Pass) -> Pass {
    let starts = late_starts(ctx, pass);
    let rules = Rules {
        starts: Some(&starts),
        people: Some(&pass.placed),
        ..Rules::default()
    };
    let mut again = Pass::new(ctx, &rules);
    again.run(ctx, &rules, None);
    again
}
// }}}

// late_starts {{{
/// Where each arc starts when all are as late as they can be: no goal later than it is, no hard
/// date passed that is kept now, and nothing before what it depends on. Arcs go in order of
/// their ends, the last first, so each makes room for what comes before it.
fn late_starts(ctx: &Context, pass: &Pass) -> Vec<i64> {
    let net = ctx.net;
    let at = |v: VertexId| pass.time[v.0 as usize].map_or(i64::MAX, |d| i64::from(d.0));
    let mut ledger = pass.ledger.clone();
    let mut limit = vec![i64::MAX; net.vertices.len()];
    for plan in &net.plans {
        let goals = plan.targets.iter().map(|t| t.vertex).chain([plan.end]);
        for goal in goals {
            limit[goal.0 as usize] = at(goal);
        }
    }
    for (i, vertex) in net.vertices.iter().enumerate() {
        if let Some(deadline) = vertex.deadline {
            let kept = i64::from(deadline.0).max(at(VertexId(i as u32)));
            limit[i] = limit[i].min(kept);
        }
    }
    let mut starts: Vec<i64> = pass
        .placed
        .iter()
        .map(|p| p.as_ref().map_or(i64::MAX, |p| i64::from(p.start.0)))
        .collect();
    let mut following: Vec<usize> = net
        .vertices
        .iter()
        .map(|v| v.next.len() + v.outgoing.len())
        .collect();
    let mut last: BinaryHeap<(i64, VertexId)> = (0..net.vertices.len())
        .filter(|i| following[*i] == 0)
        .map(|i| (at(VertexId(i as u32)), VertexId(i as u32)))
        .collect();
    while let Some((_, v)) = last.pop() {
        let vertex = net.vertex(v);
        let i = v.0 as usize;
        let next = vertex.next.iter().map(|w| limit[w.0 as usize]);
        let waits = |a: &&ArcId| net.arc(**a).work().is_none_or(|w| w.is_open());
        let outgoing = vertex.outgoing.iter().filter(waits);
        let outgoing = outgoing.map(|a| starts[a.0 as usize]);
        limit[i] = next.chain(outgoing).fold(limit[i], i64::min);
        for arc in &vertex.incoming {
            if limit[i] != i64::MAX {
                let by = Day(limit[i].clamp(0, ctx.table.days as i64) as u32);
                starts[arc.0 as usize] = push_late(ctx, pass, &mut ledger, *arc, by);
            }
        }
        let before = vertex.before();
        let before = before.chain(vertex.incoming.iter().map(|a| net.arc(*a).start));
        for u in before {
            following[u.0 as usize] -= 1;
            if following[u.0 as usize] == 0 {
                last.push((at(u), u));
            }
        }
    }
    starts
}

/// Moves an arc to end by `by` if that is later than it is, and gives where it starts
fn push_late(
    ctx: &Context,
    pass: &Pass,
    ledger: &mut crate::scheduler::ledger::Ledger,
    id: ArcId,
    by: Day,
) -> i64 {
    let Some(placed) = &pass.placed[id.0 as usize] else {
        return i64::MAX;
    };
    let stay = i64::from(placed.start.0);
    let length = placed.end.0.saturating_sub(placed.start.0);
    let work = match &ctx.net.arc(id).kind {
        ArcKind::Buffer(buffer) => {
            let start = match buffer.until {
                Until::Days(_) => by.0.saturating_sub(length),
                Until::Day(_) => by.0,
            };
            return stay.max(i64::from(start));
        }
        ArcKind::Work(work) => work,
    };
    // what has started stays, and so does what has a date to start by
    if !work.is_open() || work.latest_start.is_some() || by <= placed.end {
        return stay;
    }
    let by = match work.latest_end {
        Some(date) => by.min(date.max(placed.end)),
        None => by,
    };
    let (asks, valid) = ctx.valid(id, work, &placed.rows);
    ledger.release(ctx.table, &asks, &placed.days, id);
    let window = ledger.window_back(&valid, &asks, by, work.days, placed.start);
    let (start, end) = window.unwrap_or((placed.start, placed.end));
    let days: Vec<Day> = valid.days(start, end).collect();
    ledger.claim(&asks, &days, id);
    i64::from(start.0)
}
// }}}
