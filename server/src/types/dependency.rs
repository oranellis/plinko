// modules{{{
use std::hash::Hash;

use uuid::Uuid;
// }}}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    pub node: DependencyNode,
    pub lag: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DependencyNode {
    /// The start of the enclosing task group, or the plan start for top level items
    #[default]
    ParentStart,
    Node(PlanNode),
}

impl PlanNode {
    pub fn id(&self) -> Uuid {
        match *self {
            PlanNode::Task(id)
            | PlanNode::Milestone(id)
            | PlanNode::PlanBuffer(id)
            | PlanNode::TaskGroup(id) => id,
        }
    }
}

/// A schedulable node. A task group is a single node that ends when all of its members end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanBuffer(Uuid),
    TaskGroup(Uuid),
}
