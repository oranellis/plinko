use std::hash::Hash;

use uuid::Uuid;

pub struct Dependency {
    node: PlanNode,
    lag: LagDays,
}

type LagDays = i32;

pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanStart,
}

impl Hash for PlanNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}
