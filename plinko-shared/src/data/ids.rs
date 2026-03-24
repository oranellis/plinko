use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MilestoneId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TagId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

// ── NodeId string representation (needed for JSON map keys) ────────────────── {{{
impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NodeId::Task(id) => write!(f, "task:{}", id.0),
            NodeId::Milestone(id) => write!(f, "milestone:{}", id.0),
            NodeId::PlanStart => write!(f, "plan_start"),
        }
    }
}

impl FromStr for NodeId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "plan_start" {
            Ok(NodeId::PlanStart)
        } else if let Some(rest) = s.strip_prefix("task:") {
            Uuid::parse_str(rest)
                .map(|u| NodeId::Task(TaskId(u)))
                .map_err(|e| e.to_string())
        } else if let Some(rest) = s.strip_prefix("milestone:") {
            Uuid::parse_str(rest)
                .map(|u| NodeId::Milestone(MilestoneId(u)))
                .map_err(|e| e.to_string())
        } else {
            Err(format!("invalid NodeId string: {s}"))
        }
    }
}
// }}}
