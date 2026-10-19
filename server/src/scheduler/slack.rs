// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::{Days, NaiveDate};

use crate::{
    planner::graph::Vertex,
    scheduler::network::Network,
    types::{common::DateRange, dependency::PlanNode},
};
// }}}

/// How a vertex limits one it follows
#[derive(Clone, Copy)]
enum Link {
    /// The end of a group is the end of its last member
    Member,
    /// A start no sooner than `lag` days after the end, a milestone one day sooner
    Lag { lag: i32, milestone: bool },
}

/// The latest each vertex may end without passing the ends already known for others in
/// `initial`, working back from them over dependencies and the order people work in (`resource`,
/// pairs of tasks where the second follows the first). Vertices that nothing limits are left out.
pub fn late_ends(
    net: &Network,
    placed: &HashMap<Vertex, DateRange>,
    resource: &[(Vertex, Vertex)],
    mut late: HashMap<Vertex, NaiveDate>,
) -> HashMap<Vertex, NaiveDate> {
    let mut before: HashMap<Vertex, Vec<(Vertex, Link)>> = HashMap::new();
    for v in net.order().iter().filter(|v| placed.contains_key(v)) {
        for edge in net.deps(v) {
            let Some(from) = edge.from.filter(|f| placed.contains_key(f)) else {
                continue;
            };
            let link = match (v, from) {
                (Vertex::Node(PlanNode::TaskGroup(_)), Vertex::GroupStart(_)) => continue,
                (Vertex::Node(PlanNode::TaskGroup(_)), _) => Link::Member,
                _ => Link::Lag {
                    lag: edge.lag,
                    milestone: matches!(v, Vertex::Node(PlanNode::Milestone(_))),
                },
            };
            before.entry(*v).or_default().push((from, link));
        }
    }
    for (first, second) in resource {
        let link = Link::Lag {
            lag: 0,
            milestone: false,
        };
        before.entry(*second).or_default().push((*first, link));
    }
    // everything after a vertex must be done before it can be, so count what follows
    let mut following: HashMap<Vertex, usize> = placed.keys().map(|v| (*v, 0)).collect();
    for from in before.values().flatten().map(|(from, _)| from) {
        *following.get_mut(from).expect("placed") += 1;
    }
    let mut ready: Vec<Vertex> = following
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(v, _)| *v)
        .collect();
    while let Some(v) = ready.pop() {
        let range = placed[&v];
        for (from, link) in before.get(&v).into_iter().flatten() {
            if let Some(end) = late.get(&v).copied() {
                let limit = match link {
                    Link::Member => end,
                    Link::Lag { lag, milestone } => {
                        let length = (range.end() - range.start()).num_days();
                        let start = end - Days::new(length.unsigned_abs());
                        let allowed = start - chrono::Duration::days(i64::from(*lag));
                        allowed + chrono::Duration::days(i64::from(*milestone))
                    }
                };
                let slot = late.entry(*from).or_insert(limit);
                *slot = (*slot).min(limit);
            }
            let n = following.get_mut(from).expect("placed");
            *n -= 1;
            if *n == 0 {
                ready.push(*from);
            }
        }
    }
    late
}

/// Where a vertex would start and end if it could start on a day
pub type Span<'a> = dyn Fn(&Vertex, NaiveDate) -> Option<(NaiveDate, NaiveDate)> + 'a;

/// The earliest each vertex can start however free people are, taking the lengths in `placed`,
/// the ends of what has happened and the dates in `floor` as they are. A start in `exact` is
/// kept (work that has started or is done), the rest start no sooner than their `floor`, the
/// ends of what they depend on and the lag after them. `span` gives where a vertex would start
/// and end if it could start on a day, or `None` if it takes no time.
pub fn early_starts(
    span: &Span,
    net: &Network,
    placed: &HashMap<Vertex, DateRange>,
    exact: &HashSet<Vertex>,
    floor: &HashMap<Vertex, NaiveDate>,
) -> HashMap<Vertex, NaiveDate> {
    let mut starts: HashMap<Vertex, NaiveDate> = HashMap::new();
    let mut ends: HashMap<Vertex, NaiveDate> = HashMap::new();
    for v in net.order() {
        let Some(range) = placed.get(v) else {
            continue;
        };
        if exact.contains(v) {
            starts.insert(*v, range.start());
            ends.insert(*v, range.end());
            continue;
        }
        let is_group = matches!(v, Vertex::Node(PlanNode::TaskGroup(_)));
        let milestone = matches!(v, Vertex::Node(PlanNode::Milestone(_)));
        let mut start = floor.get(v).copied();
        let (mut first, mut last): (Option<NaiveDate>, Option<NaiveDate>) = (None, None);
        for edge in net.deps(v) {
            let Some(from) = edge.from.filter(|f| ends.contains_key(f)) else {
                continue;
            };
            if is_group {
                if !matches!(from, Vertex::GroupStart(_)) {
                    let at = starts[&from];
                    first = Some(first.map_or(at, |f| f.min(at)));
                    last = last.max(Some(ends[&from]));
                }
                continue;
            }
            let after = ends[&from] + chrono::Duration::days(i64::from(edge.lag));
            let after = after - chrono::Duration::days(i64::from(milestone));
            start = start.max(Some(after));
        }
        if is_group {
            let start = first.or(start).unwrap_or(range.start());
            starts.insert(*v, start);
            ends.insert(*v, last.unwrap_or(start));
        } else {
            let start = start.unwrap_or(range.start());
            let (start, end) = span(v, start).unwrap_or((start, start));
            starts.insert(*v, start);
            ends.insert(*v, end);
        }
    }
    starts
}
