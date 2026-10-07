// modules {{{
use std::collections::HashMap;

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
