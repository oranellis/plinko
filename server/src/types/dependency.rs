use std::hash::Hash;

use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    node: PlanNode,
    lag: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlanNode {
    #[default]
    PlanStart,
    Task(Uuid),
    Milestone(Uuid),
}
