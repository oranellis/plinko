// modules{{{
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{
    planner::items::{
        dependency::PlanNode, milestone::Milestone, parent::Parent, plan_buffer::PlanBuffer,
        plan_delay::PlanDelay, task::Task, task_group::TaskGroup,
    },
    planner::{plan::Plan, plan_entity::PlanItem},
};
// }}}

/// A task group is two vertices: its start, holding the group's own dependencies, and its end,
/// `Node(PlanNode::TaskGroup)`, which depends on the start and every member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Vertex {
    Node(PlanNode),
    GroupStart(Uuid),
}

// impl Vertex {{{
impl Vertex {
    /// So results do not depend on map order
    pub(crate) fn key(&self) -> (u8, Uuid) {
        match self {
            Vertex::GroupStart(id) => (0, *id),
            Vertex::Node(PlanNode::Task(id)) => (1, *id),
            Vertex::Node(PlanNode::Milestone(id)) => (2, *id),
            Vertex::Node(PlanNode::PlanBuffer(id)) => (3, *id),
            Vertex::Node(PlanNode::TaskGroup(id)) => (4, *id),
            Vertex::Node(PlanNode::PlanDelay(id)) => (5, *id),
        }
    }
}

impl From<PlanNode> for Vertex {
    fn from(node: PlanNode) -> Self {
        Vertex::Node(node)
    }
}
// }}}

// Vertex supporting types {{{
/// `from` is `None` for a dependency on today
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    pub from: Option<Vertex>,
}

pub(crate) struct Graph<'a> {
    plan: &'a Plan,
    members: HashMap<Uuid, Vec<PlanNode>>,
}

/// Memoised reachability between vertices, `None` standing for today
struct Reach<'a, 'p> {
    graph: &'a Graph<'p>,
    known: HashMap<(Vertex, Option<Vertex>), bool>,
}
// }}}

// impl Graph{{{
impl<'a> Graph<'a> {
    pub(crate) fn new(plan: &'a Plan) -> Self {
        let mut members: HashMap<Uuid, Vec<PlanNode>> = HashMap::new();
        collect_members::<Task>(plan, &mut members);
        collect_members::<Milestone>(plan, &mut members);
        collect_members::<PlanBuffer>(plan, &mut members);
        collect_members::<PlanDelay>(plan, &mut members);
        collect_members::<TaskGroup>(plan, &mut members);
        Self { plan, members }
    }

    fn waits_on(&self, v: Vertex) -> Vec<Vertex> {
        self.edges(v).into_iter().filter_map(|e| e.from).collect()
    }

    /// In a fixed order
    pub(crate) fn vertices(&self) -> Vec<Vertex> {
        let plan = self.plan;
        let mut all: Vec<Vertex> = [
            nodes::<Task>(plan),
            nodes::<Milestone>(plan),
            nodes::<PlanBuffer>(plan),
            nodes::<PlanDelay>(plan),
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

    pub(crate) fn edges(&self, v: Vertex) -> Vec<Edge> {
        match v {
            Vertex::GroupStart(id) => self.item_edges::<TaskGroup>(id),
            Vertex::Node(PlanNode::TaskGroup(id)) => {
                let members = self.members.get(&id).into_iter().flatten();
                let from = members.map(|n| Vertex::Node(*n));
                from.chain([Vertex::GroupStart(id)])
                    .map(|v| Edge { from: Some(v) })
                    .collect()
            }
            Vertex::Node(PlanNode::Task(id)) => self.item_edges::<Task>(id),
            Vertex::Node(PlanNode::Milestone(id)) => self.item_edges::<Milestone>(id),
            Vertex::Node(PlanNode::PlanBuffer(id)) => self.item_edges::<PlanBuffer>(id),
            Vertex::Node(PlanNode::PlanDelay(id)) => self.item_edges::<PlanDelay>(id),
        }
    }

    fn item_edges<T: PlanItem>(&self, id: Uuid) -> Vec<Edge> {
        let Some(item) = T::entities(self.plan).get(&id) else {
            return Vec::new();
        };
        // in the item's own order, then its parent's start, which every item waits for: the
        // enclosing group's start, or today at the top level. `simplified` relies on this order.
        let start = match item.parent() {
            Parent::Node(group) => Some(Vertex::GroupStart(group)),
            Parent::Plan => None,
        };
        let dependencies = item.dependencies().iter().map(|d| Some((*d).into()));
        dependencies
            .chain([start])
            .map(|from| Edge { from })
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

// }}}

// cycle check {{{
/// Only cycles through the changed node can be new, so this is all an update needs checking
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
// }}}

// nodes {{{
pub(crate) fn nodes<T: PlanItem>(plan: &Plan) -> Vec<PlanNode> {
    T::entities(plan).keys().map(|id| T::node(*id)).collect()
}
// }}}

// impl Reach {{{
impl Reach<'_, '_> {
    fn leads(&mut self, from: Vertex, to: Option<Vertex>) -> bool {
        if let Some(known) = self.known.get(&(from, to)) {
            return *known;
        }
        // provisional false stops a cycle recursing for ever
        self.known.insert((from, to), false);
        let leads = self
            .graph
            .edges(from)
            .into_iter()
            .any(|edge| edge.from == to || edge.from.is_some_and(|u| self.leads(u, to)));
        self.known.insert((from, to), leads);
        leads
    }
}
// }}}

// simplified {{{
/// Drops dependencies another dependency of the same item already implies (a→b→c makes a direct
/// a→c redundant). Returns each changed item with the dependencies to keep, in order.
pub fn simplified(plan: &Plan, touched: PlanNode) -> Vec<(PlanNode, Vec<PlanNode>)> {
    let graph = Graph::new(plan);
    let vertices = graph.vertices();
    let mut dependents: HashMap<Vertex, Vec<Vertex>> = HashMap::new();
    for v in &vertices {
        for from in graph.waits_on(*v) {
            dependents.entry(from).or_default().push(*v);
        }
    }
    // only the touched item and what waits on it can have gained a shortcut
    let mut affected: HashSet<Vertex> = HashSet::new();
    let mut pending = match touched {
        PlanNode::TaskGroup(id) => vec![Vertex::GroupStart(id), touched.into()],
        _ => vec![touched.into()],
    };
    while let Some(v) = pending.pop() {
        if affected.insert(v) {
            pending.extend(dependents.get(&v).into_iter().flatten());
        }
    }
    let mut reach = Reach {
        graph: &graph,
        known: HashMap::new(),
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
            Vertex::Node(node @ PlanNode::PlanDelay(id)) => {
                (node, dependencies::<PlanDelay>(plan, id))
            }
            Vertex::Node(PlanNode::TaskGroup(_)) => continue,
        };
        let edges = graph.edges(v);
        let kept: Vec<PlanNode> = deps
            .iter()
            .zip(&edges)
            .enumerate()
            .filter(|(i, (_, edge))| {
                !edges.iter().enumerate().any(|(j, other)| {
                    let Some(source) = other.from else {
                        return false;
                    };
                    j != *i && other.from != edge.from && reach.leads(source, edge.from)
                })
            })
            .map(|(_, (dep, _))| *dep)
            .collect();
        if kept.len() < deps.len() {
            changed.push((item, kept));
        }
    }
    changed
}
// }}}

// dependencies {{{
fn dependencies<'a, T: PlanItem + 'a>(plan: &'a Plan, id: Uuid) -> &'a [PlanNode] {
    T::entities(plan)
        .get(&id)
        .map_or(&[], |item| item.dependencies())
}
// }}}
