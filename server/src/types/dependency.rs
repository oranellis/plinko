// modules{{{
use std::hash::Hash;

use uuid::Uuid;
// }}}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    node: DependencyNode,
    lag: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum DependencyNode {
    /// The start of the enclosing task group, or the plan start for top level items
    #[default]
    ParentStart,
    Node(PlanNode),
}

/// A schedulable node. A task group is a single node that ends when all of its members end.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanBuffer(Uuid),
    TaskGroup(Uuid),
}
