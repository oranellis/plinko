// modules{{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    planner::plan::Plan,
    types::{
        dependency::{Dependency, PlanNode},
        milestone::Milestone,
        parent::Parent,
        plan_buffer::PlanBuffer,
        task::{Task, Worker},
        task_group::TaskGroup,
    },
};
// }}}

/// A schedulable item stored in a plan: it has a parent, dependencies and a node identity
pub trait PlanItem: Sized + Clone {
    fn node(id: Uuid) -> PlanNode;
    fn items(plan: &Plan) -> &HashMap<Uuid, Self>;
    fn items_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self>;
    fn parent(&self) -> Parent;
    fn dependencies(&self) -> &[Dependency];
    fn workers(&self) -> Vec<Worker> {
        Vec::new()
    }
}

macro_rules! plan_item {
    ($ty:ident, $node:ident, $field:ident $(, workers: $workers:expr)?) => {
        impl PlanItem for $ty {
            fn node(id: Uuid) -> PlanNode {
                PlanNode::$node(id)
            }
            fn items(plan: &Plan) -> &HashMap<Uuid, Self> {
                &plan.$field
            }
            fn items_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
                &mut plan.$field
            }
            fn parent(&self) -> Parent {
                $ty::parent(self)
            }
            fn dependencies(&self) -> &[Dependency] {
                $ty::dependencies(self)
            }
            $(fn workers(&self) -> Vec<Worker> {
                $workers(self)
            })?
        }
    };
}

plan_item!(Task, Task, tasks, workers: |t: &Task| t.allocations().keys().copied().collect());
plan_item!(Milestone, Milestone, milestones);
plan_item!(PlanBuffer, PlanBuffer, plan_buffers);
plan_item!(TaskGroup, TaskGroup, task_groups);
