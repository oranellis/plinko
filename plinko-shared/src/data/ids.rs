use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MilestoneId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeId {
    Task(TaskId),
    Milestone(MilestoneId),
    PlanStart,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl MilestoneId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MilestoneId {
    fn default() -> Self {
        Self::new()
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TagId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TagId {
    fn default() -> Self {
        Self::new()
    }
}
// }}}
