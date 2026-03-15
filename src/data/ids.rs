use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MilestoneId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeId {
    Task(TaskId),
    Milestone(MilestoneId),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagId(pub Uuid);

impl TagId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
