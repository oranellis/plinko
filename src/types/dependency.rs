use std::hash::Hash;

use crate::types::{milestone::MilestoneUuid, task::TaskUuid};

pub struct Dependency {
    node: PlanNode,
    lag: LagDays,
}

type LagDays = i32;

pub enum PlanNode {
    Task(TaskUuid),
    Milestone(MilestoneUuid),
    PlanStart,
}

impl Hash for PlanNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}
