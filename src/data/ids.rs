//! Newtype wrappers around [`uuid::Uuid`] for domain entity identifiers.
//!
//! Using distinct types for [`TaskId`], [`MilestoneId`], and [`UserId`] lets
//! the compiler catch accidental mix-ups at compile time.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a [`Task`](super::Task).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

/// Unique identifier for a [`Milestone`](super::Milestone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MilestoneId(pub Uuid);

/// Unique identifier for a [`User`](super::User).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

/// A node can point to a Task, a Milestone, or the Plan's own start date. Used for dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeId {
    Task(TaskId),
    Milestone(MilestoneId),
    /// The plan's start date — acts as the root anchor for the whole schedule.
    PlanStart,
}

impl TaskId {
    /// Generates a new random [`TaskId`].
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl MilestoneId {
    /// Generates a new random [`MilestoneId`].
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl UserId {
    /// Generates a new random [`UserId`].
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
    fn node_id_equality() {
        let tid = TaskId::new();
        let mid = MilestoneId::new();
        assert_eq!(NodeId::Task(tid), NodeId::Task(tid));
        assert_eq!(NodeId::Milestone(mid), NodeId::Milestone(mid));
        assert_ne!(NodeId::Task(tid), NodeId::Milestone(mid));
        assert_ne!(NodeId::PlanStart, NodeId::Task(tid));
        assert_eq!(NodeId::PlanStart, NodeId::PlanStart);
    }
}
