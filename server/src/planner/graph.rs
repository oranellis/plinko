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
enum Vertex {
    Node(PlanNode),
    GroupStart(Uuid),
}

impl From<PlanNode> for Vertex {
    fn from(node: PlanNode) -> Self {
        Vertex::Node(node)
    }
}

struct Graph<'a> {
    plan: &'a Plan,
    members: HashMap<Uuid, Vec<PlanNode>>,
}

// impl Graph{{{
impl<'a> Graph<'a> {
    fn new(plan: &'a Plan) -> Self {
        let mut members: HashMap<Uuid, Vec<PlanNode>> = HashMap::new();
        collect_members::<Task>(plan, &mut members);
        collect_members::<Milestone>(plan, &mut members);
        collect_members::<PlanBuffer>(plan, &mut members);
        collect_members::<TaskGroup>(plan, &mut members);
        Self { plan, members }
    }

    /// The vertices that `v` waits on
    fn waits_on(&self, v: Vertex) -> Vec<Vertex> {
        self.edges(v).into_iter().flatten().collect()
    }

    /// What `v` waits on, where `None` is the plan start
    fn edges(&self, v: Vertex) -> Vec<Option<Vertex>> {
        match v {
            Vertex::GroupStart(id) => self.item_edges::<TaskGroup>(id),
            Vertex::Node(PlanNode::TaskGroup(id)) => {
                let members = self.members.get(&id).into_iter().flatten();
                members
                    .map(|n| Some(Vertex::Node(*n)))
                    .chain([Some(Vertex::GroupStart(id))])
                    .collect()
            }
            Vertex::Node(PlanNode::Task(id)) => self.item_edges::<Task>(id),
            Vertex::Node(PlanNode::Milestone(id)) => self.item_edges::<Milestone>(id),
            Vertex::Node(PlanNode::PlanBuffer(id)) => self.item_edges::<PlanBuffer>(id),
        }
    }

    fn item_edges<T: PlanItem>(&self, id: Uuid) -> Vec<Option<Vertex>> {
        let Some(item) = T::entities(self.plan).get(&id) else {
            return Vec::new();
        };
        let parent = item.parent();
        item.dependencies()
            .iter()
            .map(|d| resolve(d, parent))
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

/// Nodes with no chain of dependencies back to the plan start. A node is anchored if any of its
/// dependencies is the plan start or an anchored node.
pub fn unscheduled(plan: &Plan) -> HashSet<PlanNode> {
    let graph = Graph::new(plan);
    let all_nodes: Vec<PlanNode> = [
        nodes::<Task>(plan),
        nodes::<Milestone>(plan),
        nodes::<PlanBuffer>(plan),
        nodes::<TaskGroup>(plan),
    ]
    .concat();
    let group_starts = plan.task_groups().keys().map(|id| Vertex::GroupStart(*id));
    let vertices: Vec<Vertex> = all_nodes
        .iter()
        .map(|n| Vertex::Node(*n))
        .chain(group_starts)
        .collect();

    let mut dependents: HashMap<Vertex, Vec<Vertex>> = HashMap::new();
    let mut anchored = HashSet::new();
    let mut pending = Vec::new();
    for v in &vertices {
        let edges = graph.edges(*v);
        if edges.contains(&None) && anchored.insert(*v) {
            pending.push(*v);
        }
        for dependency in edges.into_iter().flatten() {
            dependents.entry(dependency).or_default().push(*v);
        }
    }
    while let Some(v) = pending.pop() {
        for dependent in dependents.get(&v).into_iter().flatten() {
            if anchored.insert(*dependent) {
                pending.push(*dependent);
            }
        }
    }
    all_nodes
        .into_iter()
        .filter(|n| !anchored.contains(&Vertex::Node(*n)))
        .collect()
}
