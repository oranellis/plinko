// modules{{{
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{
    planner::{plan::Plan, plan_entity::PlanItem},
    types::{
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::Milestone,
        parent::Parent,
        plan_buffer::PlanBuffer,
        task::Task,
        task_group::TaskGroup,
    },
};
// }}}

/// A point in the dependency graph. A task group has two: its start (the group's own dependencies)
/// and its end, which is `Node(PlanNode::TaskGroup)` and depends on the start and every member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Vertex {
    Node(PlanNode),
    GroupStart(Uuid),
}

impl Vertex {
    /// A total order over vertices, for results that must not depend on map order
    pub(crate) fn key(&self) -> (u8, Uuid) {
        match self {
            Vertex::GroupStart(id) => (0, *id),
            Vertex::Node(PlanNode::Task(id)) => (1, *id),
            Vertex::Node(PlanNode::Milestone(id)) => (2, *id),
            Vertex::Node(PlanNode::PlanBuffer(id)) => (3, *id),
            Vertex::Node(PlanNode::TaskGroup(id)) => (4, *id),
        }
    }
}

/// A dependency of a vertex, on the plan start when `from` is `None`
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    pub from: Option<Vertex>,
    pub lag: i32,
}

impl From<PlanNode> for Vertex {
    fn from(node: PlanNode) -> Self {
        Vertex::Node(node)
    }
}

pub(crate) struct Graph<'a> {
    plan: &'a Plan,
    members: HashMap<Uuid, Vec<PlanNode>>,
}

// impl Graph{{{
impl<'a> Graph<'a> {
    pub(crate) fn new(plan: &'a Plan) -> Self {
        let mut members: HashMap<Uuid, Vec<PlanNode>> = HashMap::new();
        collect_members::<Task>(plan, &mut members);
        collect_members::<Milestone>(plan, &mut members);
        collect_members::<PlanBuffer>(plan, &mut members);
        collect_members::<TaskGroup>(plan, &mut members);
        Self { plan, members }
    }

    /// The vertices that `v` waits on
    fn waits_on(&self, v: Vertex) -> Vec<Vertex> {
        self.edges(v).into_iter().filter_map(|e| e.from).collect()
    }

    /// The items whose parent is the group
    pub(crate) fn members(&self, group: &Uuid) -> &[PlanNode] {
        self.members.get(group).map_or(&[], Vec::as_slice)
    }

    /// Every vertex of the plan, in a fixed order
    pub(crate) fn vertices(&self) -> Vec<Vertex> {
        let plan = self.plan;
        let mut all: Vec<Vertex> = [
            nodes::<Task>(plan),
            nodes::<Milestone>(plan),
            nodes::<PlanBuffer>(plan),
            nodes::<TaskGroup>(plan),
        ]
        .concat()
        .into_iter()
        .map(Vertex::Node)
        .chain(plan.task_groups().keys().map(|id| Vertex::GroupStart(*id)))
        .collect();
        all.sort_by_key(Vertex::key);
        all
    }

    /// What `v` waits on
    pub(crate) fn edges(&self, v: Vertex) -> Vec<Edge> {
        match v {
            Vertex::GroupStart(id) => self.item_edges::<TaskGroup>(id),
            Vertex::Node(PlanNode::TaskGroup(id)) => {
                let members = self.members.get(&id).into_iter().flatten();
                let from = members.map(|n| Vertex::Node(*n));
                from.chain([Vertex::GroupStart(id)])
                    .map(|v| Edge {
                        from: Some(v),
                        lag: 0,
                    })
                    .collect()
            }
            Vertex::Node(PlanNode::Task(id)) => self.item_edges::<Task>(id),
            Vertex::Node(PlanNode::Milestone(id)) => self.item_edges::<Milestone>(id),
            Vertex::Node(PlanNode::PlanBuffer(id)) => self.item_edges::<PlanBuffer>(id),
        }
    }

    fn item_edges<T: PlanItem>(&self, id: Uuid) -> Vec<Edge> {
        let Some(item) = T::entities(self.plan).get(&id) else {
            return Vec::new();
        };
        let parent = item.parent();
        item.dependencies()
            .iter()
            .map(|d| Edge {
                from: resolve(d, parent),
                lag: d.lag,
            })
            .collect()
    }
}

fn collect_members<T: PlanItem>(plan: &Plan, members: &mut HashMap<Uuid, Vec<PlanNode>>) {
    for (id, item) in T::entities(plan) {
        if let Parent::Node(group) = item.parent() {
            members.entry(group).or_default().push(T::node(*id));
        }
    }
}

/// `ParentStart` is the enclosing group's start, or the plan start (`None`) at top level
fn resolve(dep: &Dependency, parent: Parent) -> Option<Vertex> {
    match (dep.node, parent) {
        (DependencyNode::Node(n), _) => Some(n.into()),
        (DependencyNode::ParentStart, Parent::Node(group)) => Some(Vertex::GroupStart(group)),
        (DependencyNode::ParentStart, Parent::PlanStart) => None,
    }
}
// }}}

/// Whether any dependency path leads from `node` back to itself. Only cycles through the changed
/// node can be new, so this is all that needs checking after an update.
pub fn has_cycle_through(plan: &Plan, node: PlanNode) -> bool {
    let graph = Graph::new(plan);
    let roots: Vec<Vertex> = match node {
        PlanNode::TaskGroup(id) => vec![Vertex::GroupStart(id), node.into()],
        _ => vec![node.into()],
    };
    roots.into_iter().any(|root| graph.reaches_itself(root))
}

impl Graph<'_> {
    fn reaches_itself(&self, root: Vertex) -> bool {
        let mut stack = self.waits_on(root);
        let mut seen = HashSet::new();
        while let Some(v) = stack.pop() {
            if v == root {
                return true;
            }
            if seen.insert(v) {
                stack.extend(self.waits_on(v));
            }
        }
        false
    }
}

fn nodes<T: PlanItem>(plan: &Plan) -> Vec<PlanNode> {
    T::entities(plan).keys().map(|id| T::node(*id)).collect()
}

/// Vertices with a chain of dependencies back to the plan start, a vertex is anchored if any of
/// its dependencies is the plan start or an anchored vertex.
pub(crate) fn anchored(plan: &Plan) -> HashSet<Vertex> {
    let graph = Graph::new(plan);
    let mut dependents: HashMap<Vertex, Vec<Vertex>> = HashMap::new();
    let mut anchored = HashSet::new();
    let mut pending = Vec::new();
    for v in graph.vertices() {
        let edges = graph.edges(v);
        if edges.iter().any(|e| e.from.is_none()) && anchored.insert(v) {
            pending.push(v);
        }
        for dependency in edges.into_iter().filter_map(|e| e.from) {
            dependents.entry(dependency).or_default().push(v);
        }
    }
    while let Some(v) = pending.pop() {
        for dependent in dependents.get(&v).into_iter().flatten() {
            if anchored.insert(*dependent) {
                pending.push(*dependent);
            }
        }
    }
    anchored
}

/// How much lag the longest chain of dependencies from `from` back to `to` (the plan start when
/// `None`) adds up to, with the results kept for the next question about the same `to`
struct Chains<'a, 'p> {
    graph: &'a Graph<'p>,
    longest: HashMap<(Vertex, Option<Vertex>), Option<i64>>,
}

impl Chains<'_, '_> {
    fn lag(&mut self, from: Vertex, to: Option<Vertex>) -> Option<i64> {
        if let Some(known) = self.longest.get(&(from, to)) {
            return *known;
        }
        // a cycle would loop for ever, and is not a chain
        self.longest.insert((from, to), None);
        let mut best = None;
        for edge in self.graph.edges(from) {
            let lag = i64::from(edge.lag);
            let through = if edge.from == to {
                Some(lag)
            } else {
                edge.from.and_then(|u| self.lag(u, to)).map(|l| l + lag)
            };
            best = best.max(through);
        }
        self.longest.insert((from, to), best);
        best
    }
}

/// The dependencies the items touching `touched` can do without: those another dependency of the
/// same item already implies. A chain a to b to c makes a direct a to c redundant when the chain's
/// lags add up to at least its lag. Gives each changed item with the dependencies to keep, in order.
pub fn simplified(plan: &Plan, touched: PlanNode) -> Vec<(PlanNode, Vec<Dependency>)> {
    let graph = Graph::new(plan);
    let vertices = graph.vertices();
    let mut waiting: HashMap<Vertex, Vec<Vertex>> = HashMap::new();
    for v in &vertices {
        for from in graph.waits_on(*v) {
            waiting.entry(from).or_default().push(*v);
        }
    }
    // everything that waits on the touched item, itself included, can have gained a shortcut
    let mut affected: HashSet<Vertex> = HashSet::new();
    let mut pending = match touched {
        PlanNode::TaskGroup(id) => vec![Vertex::GroupStart(id), touched.into()],
        _ => vec![touched.into()],
    };
    while let Some(v) = pending.pop() {
        if affected.insert(v) {
            pending.extend(waiting.get(&v).into_iter().flatten());
        }
    }
    let mut chains = Chains {
        graph: &graph,
        longest: HashMap::new(),
    };
    let mut changed = Vec::new();
    for v in vertices.into_iter().filter(|v| affected.contains(v)) {
        let (item, deps) = match v {
            Vertex::GroupStart(id) => {
                (PlanNode::TaskGroup(id), dependencies::<TaskGroup>(plan, id))
            }
            Vertex::Node(node @ PlanNode::Task(id)) => (node, dependencies::<Task>(plan, id)),
            Vertex::Node(node @ PlanNode::Milestone(id)) => {
                (node, dependencies::<Milestone>(plan, id))
            }
            Vertex::Node(node @ PlanNode::PlanBuffer(id)) => {
                (node, dependencies::<PlanBuffer>(plan, id))
            }
            Vertex::Node(PlanNode::TaskGroup(_)) => continue,
        };
        let edges = graph.edges(v);
        let kept: Vec<Dependency> = deps
            .iter()
            .zip(&edges)
            .enumerate()
            .filter(|(i, (_, edge))| {
                !edges.iter().enumerate().any(|(j, other)| {
                    let Some(source) = other.from else {
                        return false;
                    };
                    j != *i
                        && other.from != edge.from
                        && chains
                            .lag(source, edge.from)
                            .is_some_and(|l| i64::from(other.lag) + l >= i64::from(edge.lag))
                })
            })
            .map(|(_, (dep, _))| dep.clone())
            .collect();
        if kept.len() < deps.len() {
            changed.push((item, kept));
        }
    }
    changed
}

fn dependencies<'a, T: PlanItem + 'a>(plan: &'a Plan, id: Uuid) -> &'a [Dependency] {
    T::entities(plan)
        .get(&id)
        .map_or(&[], |item| item.dependencies())
}

/// Nodes with no chain of dependencies back to the plan start
pub fn unscheduled(plan: &Plan) -> HashSet<PlanNode> {
    let anchored = anchored(plan);
    Graph::new(plan)
        .vertices()
        .into_iter()
        .filter_map(|v| match v {
            Vertex::Node(n) if !anchored.contains(&v) => Some(n),
            _ => None,
        })
        .collect()
}
