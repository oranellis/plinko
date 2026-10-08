//! Taking an item out of a plan without breaking the chains through it: what waited on it waits
//! on what it waited on, and a group's members move up into the group's parent.

use uuid::Uuid;

use crate::{
    planner::{graph, plan::Plan, plan_entity::PlanItem, plan_update::PlanUpdate},
    types::{
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        milestone::Milestone,
        parent::Parent,
        plan_buffer::PlanBuffer,
        task::Task,
        task_group::TaskGroup,
    },
};

fn placed<T: PlanItem>(plan: &Plan, id: Uuid) -> (Parent, Vec<Dependency>) {
    T::entities(plan)
        .get(&id)
        .map_or((Parent::PlanStart, Vec::new()), |item| {
            (item.parent(), item.dependencies().to_vec())
        })
}

/// Where an item is, and what it depends on
fn placement(plan: &Plan, node: PlanNode) -> (Parent, Vec<Dependency>) {
    match node {
        PlanNode::Task(id) => placed::<Task>(plan, id),
        PlanNode::Milestone(id) => placed::<Milestone>(plan, id),
        PlanNode::PlanBuffer(id) => placed::<PlanBuffer>(plan, id),
        PlanNode::TaskGroup(id) => placed::<TaskGroup>(plan, id),
    }
}

fn all_nodes(plan: &Plan) -> Vec<PlanNode> {
    let mut nodes = [
        graph::nodes::<Task>(plan),
        graph::nodes::<Milestone>(plan),
        graph::nodes::<PlanBuffer>(plan),
        graph::nodes::<TaskGroup>(plan),
    ]
    .concat();
    nodes.sort_by_key(|n| n.id());
    nodes
}

/// Each node once, with the longest lag it was given
fn merged(deps: Vec<Dependency>) -> Vec<Dependency> {
    let mut out: Vec<Dependency> = Vec::new();
    for dep in deps {
        match out.iter_mut().find(|d| d.node == dep.node) {
            Some(known) => known.lag = known.lag.max(dep.lag),
            None => out.push(dep),
        }
    }
    out
}

impl Plan {
    /// The updates that remove `node` and leave the rest of the plan connected, the last of them
    /// removes it. Whatever depended on it depends on what it depended on, with the lags added.
    /// For a group the members move up into its parent, taking its dependencies as their start,
    /// and what depended on the group depends on each member.
    pub fn removal(&self, node: PlanNode) -> Vec<PlanUpdate> {
        let (parent, deps) = placement(self, node);
        let nodes = all_nodes(self);
        let members: Vec<PlanNode> = match node {
            PlanNode::TaskGroup(id) => nodes
                .iter()
                .copied()
                .filter(|n| placement(self, *n).0 == Parent::Node(id))
                .collect(),
            _ => Vec::new(),
        };
        let mut updates = Vec::new();

        // the members take the group's dependencies as the start they had in it
        for member in &members {
            let (_, own) = placement(self, *member);
            let start: Vec<Dependency> = if deps.is_empty() {
                vec![Dependency::default()]
            } else {
                deps.clone()
            };
            let moved = own
                .into_iter()
                .flat_map(|d| match d.node {
                    DependencyNode::ParentStart => start
                        .iter()
                        .map(|s| Dependency {
                            node: s.node,
                            lag: s.lag + d.lag,
                        })
                        .collect(),
                    DependencyNode::Node(_) => vec![d],
                })
                .collect();
            updates.push(PlanUpdate::rewire(
                *member,
                Some(parent),
                Some(merged(moved)),
            ));
        }

        // what waited on it waits on what it stood for
        let mine = DependencyNode::Node(node);
        for other in nodes.iter().filter(|n| **n != node) {
            let (other_parent, own) = placement(self, *other);
            if !own.iter().any(|d| d.node == mine) {
                continue;
            }
            let replaced: Vec<Dependency> = own
                .into_iter()
                .flat_map(|d| {
                    if d.node != mine {
                        return vec![d];
                    }
                    if !members.is_empty() {
                        return members
                            .iter()
                            .map(|m| Dependency {
                                node: DependencyNode::Node(*m),
                                lag: d.lag,
                            })
                            .collect();
                    }
                    deps.iter()
                        .filter(|s| s.node != DependencyNode::ParentStart || other_parent == parent)
                        .map(|s| Dependency {
                            node: s.node,
                            lag: s.lag + d.lag,
                        })
                        .collect()
                })
                .collect();
            let replaced = match merged(replaced) {
                kept if kept.is_empty() => vec![Dependency::default()],
                kept => kept,
            };
            updates.push(PlanUpdate::rewire(*other, None, Some(replaced)));
        }

        let mut targets: Vec<Uuid> = self
            .targets()
            .iter()
            .filter(|(_, t)| t.node() == node)
            .map(|(id, _)| *id)
            .collect();
        targets.sort();
        updates.extend(
            targets
                .into_iter()
                .map(|id| PlanUpdate::Target(Crud::Remove(id))),
        );
        updates.push(PlanUpdate::remove(node));
        updates
    }
}
