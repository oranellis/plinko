// modules {{{
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    planner::{
        graph::{Edge, Graph, Vertex},
        plan::Plan,
    },
    types::dependency::PlanNode,
};
// }}}

/// A plan's dependency graph with what the scheduler orders by: a topological order and the
/// length of the longest chain of work that still follows each vertex
pub struct Network<'a> {
    graph: Graph<'a>,
    deps: HashMap<Vertex, Vec<Edge>>,
    order: Vec<Vertex>,
    remaining: HashMap<Vertex, i64>,
    /// What waits for each vertex
    after: HashMap<Vertex, Vec<Vertex>>,
}

// impl Network {{{
impl<'a> Network<'a> {
    pub fn new(plan: &'a Plan) -> Self {
        let graph = Graph::new(plan);
        let vertices = graph.vertices();
        let deps: HashMap<Vertex, Vec<Edge>> =
            vertices.iter().map(|v| (*v, graph.edges(*v))).collect();
        let mut dependents: HashMap<Vertex, Vec<(Vertex, i32)>> = HashMap::new();
        for (v, edges) in &deps {
            for e in edges {
                if let Some(from) = e.from {
                    dependents.entry(from).or_default().push((*v, e.lag));
                }
            }
        }
        let order = topological(&vertices, &deps, &dependents);
        let mut remaining: HashMap<Vertex, i64> = HashMap::new();
        for v in order.iter().rev() {
            let after = dependents.get(v).into_iter().flatten();
            let longest = after.map(|(w, lag)| i64::from(*lag).max(0) + remaining[w]);
            let own = duration(plan, v);
            remaining.insert(*v, own + longest.max().unwrap_or(0));
        }
        let after = dependents
            .into_iter()
            .map(|(v, list)| (v, list.into_iter().map(|(w, _)| w).collect()))
            .collect();
        Self {
            graph,
            deps,
            order,
            remaining,
            after,
        }
    }

    /// Every vertex after the ones it depends on, ties broken by id
    pub fn order(&self) -> &[Vertex] {
        &self.order
    }

    pub fn deps(&self, v: &Vertex) -> &[Edge] {
        self.deps.get(v).map_or(&[], Vec::as_slice)
    }

    pub fn members(&self, group: &uuid::Uuid) -> &[PlanNode] {
        self.graph.members(group)
    }

    /// The days of work that follow the vertex and including it, the longer the more urgent
    pub fn remaining(&self, v: &Vertex) -> i64 {
        self.remaining.get(v).copied().unwrap_or(0)
    }

    /// The vertices and everything that waits for them, directly or not
    pub fn downstream(&self, roots: impl IntoIterator<Item = Vertex>) -> HashSet<Vertex> {
        let mut seen = HashSet::new();
        let mut stack: Vec<Vertex> = roots.into_iter().collect();
        while let Some(v) = stack.pop() {
            if seen.insert(v) {
                stack.extend(self.after.get(&v).into_iter().flatten().copied());
            }
        }
        seen
    }

    /// The vertices and everything they depend on
    pub fn ancestors(&self, roots: impl IntoIterator<Item = Vertex>) -> HashSet<Vertex> {
        let mut seen = HashSet::new();
        let mut stack: Vec<Vertex> = roots.into_iter().collect();
        while let Some(v) = stack.pop() {
            if seen.insert(v) {
                stack.extend(self.deps(&v).iter().filter_map(|e| e.from));
            }
        }
        seen
    }
}
// }}}

// fn duration {{{
fn duration(plan: &Plan, v: &Vertex) -> i64 {
    match v {
        Vertex::Node(PlanNode::Task(id)) => i64::from(plan.tasks()[id].duration()),
        Vertex::Node(PlanNode::PlanBuffer(id)) => i64::from(*plan.plan_buffers()[id].duration()),
        _ => 0,
    }
}
// }}}

// fn topological {{{
/// Kahn's algorithm taking the lowest id first, so the order does not depend on map order
fn topological(
    vertices: &[Vertex],
    deps: &HashMap<Vertex, Vec<Edge>>,
    dependents: &HashMap<Vertex, Vec<(Vertex, i32)>>,
) -> Vec<Vertex> {
    let mut waiting: HashMap<Vertex, usize> = vertices
        .iter()
        .map(|v| (*v, deps[v].iter().filter(|e| e.from.is_some()).count()))
        .collect();
    let mut ready: BTreeMap<(u8, uuid::Uuid), Vertex> = waiting
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(v, _)| (v.key(), *v))
        .collect();
    let mut order = Vec::with_capacity(vertices.len());
    while let Some((_, v)) = ready.pop_first() {
        order.push(v);
        for (w, _) in dependents.get(&v).into_iter().flatten() {
            let n = waiting.get_mut(w).expect("every vertex is counted");
            *n -= 1;
            if *n == 0 {
                ready.insert(w.key(), *w);
            }
        }
    }
    order
}
// }}}
