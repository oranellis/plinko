use std::hash::Hash;

use uuid::Uuid;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    node: PlanNode,
    lag: i32,
}

#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub enum PlanNode {
    #[default]
    PlanStart,
    Task(Uuid),
    Milestone(Uuid),
}
