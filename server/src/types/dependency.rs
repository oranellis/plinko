use std::hash::Hash;

use uuid::Uuid;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    node: PlanNode,
    lag: LagDays,
}

type LagDays = i32;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum PlanNode {
    #[default]
    PlanStart,
    Task(Uuid),
    Milestone(Uuid),
}

impl Hash for PlanNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}
