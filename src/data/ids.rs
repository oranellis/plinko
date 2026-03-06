use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MilestoneId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

/// A dependency can point to a Task, a Milestone, or the Plan's own start date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyId {
    Task(TaskId),
    Milestone(MilestoneId),
    /// The plan's start date — acts as the root anchor for the whole schedule.
    PlanStart,
}

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl MilestoneId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_ids_are_unique() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn milestone_ids_are_unique() {
        let a = MilestoneId::new();
        let b = MilestoneId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn user_ids_are_unique() {
        let a = UserId::new();
        let b = UserId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn dependency_id_equality() {
        let tid = TaskId::new();
        let mid = MilestoneId::new();
        assert_eq!(DependencyId::Task(tid), DependencyId::Task(tid));
        assert_eq!(DependencyId::Milestone(mid), DependencyId::Milestone(mid));
        assert_ne!(DependencyId::Task(tid), DependencyId::Milestone(mid));
        assert_ne!(DependencyId::PlanStart, DependencyId::Task(tid));
        assert_eq!(DependencyId::PlanStart, DependencyId::PlanStart);
    }
}
