//! Removing an item without breaking the chains through it: what waited on it waits on what it
//! waited on, and a group's members move up into the group's parent.

// modules {{{
use uuid::Uuid;

use crate::{
    common::crud::Crud,
    planner::items::{
        dependency::PlanNode, milestone::Milestone, parent::Parent, plan_buffer::PlanBuffer,
        plan_delay::PlanDelay, task::Task, task_group::TaskGroup,
    },
    planner::{
        graph,
        plan::{Plan, PlanSchedulerDataPatch},
        plan_entity::PlanItem,
        plan_update::PlanUpdate,
    },
};
// }}}

// helpers {{{
fn placed<T: PlanItem>(plan: &Plan, id: Uuid) -> (Parent, Vec<PlanNode>) {
    T::entities(plan)
        .get(&id)
        .map_or((Parent::Plan, Vec::new()), |item| {
            (item.parent(), item.dependencies().to_vec())
        })
}

pub(super) fn placement(plan: &Plan, node: PlanNode) -> (Parent, Vec<PlanNode>) {
    match node {
        PlanNode::Task(id) => placed::<Task>(plan, id),
        PlanNode::Milestone(id) => placed::<Milestone>(plan, id),
        PlanNode::PlanBuffer(id) => placed::<PlanBuffer>(plan, id),
        PlanNode::PlanDelay(id) => placed::<PlanDelay>(plan, id),
        PlanNode::TaskGroup(id) => placed::<TaskGroup>(plan, id),
    }
}

pub(super) fn all_nodes(plan: &Plan) -> Vec<PlanNode> {
    let mut nodes = [
        graph::nodes::<Task>(plan),
        graph::nodes::<Milestone>(plan),
        graph::nodes::<PlanBuffer>(plan),
        graph::nodes::<PlanDelay>(plan),
        graph::nodes::<TaskGroup>(plan),
    ]
    .concat();
    nodes.sort_by_key(|n| n.id());
    nodes
}
// }}}

// impl Plan {{{
impl Plan {
    /// The last update is the removal itself. What depended on a group depends on each member.
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

        // members take the group's dependencies, unless they wait on another member which does
        for member in &members {
            let (_, own) = placement(self, *member);
            let moved = match own.iter().any(|d| members.contains(d)) {
                true => own,
                false => PlanNode::merged([own, deps.clone()].concat()),
            };
            updates.push(PlanUpdate::rewire(*member, Some(parent), Some(moved)));
        }

        for other in nodes.iter().filter(|n| **n != node) {
            let (_, own) = placement(self, *other);
            if !own.contains(&node) {
                continue;
            }
            let replaced: Vec<PlanNode> = own
                .into_iter()
                .flat_map(|d| {
                    if d != node {
                        return vec![d];
                    }
                    match members.is_empty() {
                        true => deps.clone(),
                        false => members.clone(),
                    }
                })
                .collect();
            let replaced = PlanNode::merged(replaced);
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
        if self.scheduler_data().default_start() == Some(node) {
            updates.push(PlanUpdate::SchedulerData(PlanSchedulerDataPatch {
                priority: None,
                default_start: Some(None),
            }));
        }
        updates.push(PlanUpdate::remove(node));
        updates
    }
}
// }}}
