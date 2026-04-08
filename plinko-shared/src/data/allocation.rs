use std::collections::HashMap;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::data::constraint::ConstraintKind;
use crate::data::ids::{MilestoneId, NodeId, TaskId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSegment {
    pub user: UserId,
    pub date: NaiveDate,
    pub hours_worked: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    NotStarted,
    InProgress,
    OnHold,
    Complete,
    Dropped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub status: Status,
    pub allocation: TaskAllocation,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TaskState {
    pub fn not_started() -> Self {
        let sentinel = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        Self {
            status: Status::NotStarted,
            allocation: TaskAllocation::Dynamic {
                scheduled_start_date: sentinel,
                scheduled_end_date: sentinel,
                time_allocation: vec![],
            },
        }
    }
}
// }}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskAllocation {
    // Fixed allocations must not change between scheduling runs
    Fixed {
        start_date: NaiveDate,
        end_date: NaiveDate,
        corrected_end_date: Option<NaiveDate>,
        time_allocation: Vec<WorkSegment>,
    },
    Dynamic {
        scheduled_start_date: NaiveDate,
        scheduled_end_date: NaiveDate,
        time_allocation: Vec<WorkSegment>,
    },
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TaskAllocation {
    pub fn start_date(&self) -> NaiveDate {
        match self {
            TaskAllocation::Fixed { start_date, .. } => *start_date,
            TaskAllocation::Dynamic {
                scheduled_start_date,
                ..
            } => *scheduled_start_date,
        }
    }

    pub fn end_date(&self) -> NaiveDate {
        match self {
            TaskAllocation::Fixed {
                end_date,
                corrected_end_date,
                ..
            } => corrected_end_date.unwrap_or(*end_date),
            TaskAllocation::Dynamic {
                scheduled_end_date, ..
            } => *scheduled_end_date,
        }
    }
}
// }}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneAllocation {
    pub date: NaiveDate,
    derived_status: Status,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl MilestoneAllocation {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            derived_status: Status::NotStarted,
        }
    }

    pub fn date(&self) -> NaiveDate {
        self.date
    }

    pub fn derived_status(&self) -> Status {
        self.derived_status
    }

    pub fn set_derived_status(&mut self, status: Status) {
        self.derived_status = status;
    }
}
// }}}

/// Records that a task or milestone could not meet its scheduled constraint
/// and was pushed to the earliest possible date instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintViolation {
    pub node_name: String,
    pub kind: ConstraintKind,
    pub required_date: NaiveDate,
    pub scheduled_date: NaiveDate,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl ConstraintViolation {
    pub fn message(&self) -> String {
        match self.kind {
            ConstraintKind::Fixed => format!(
                "\"{}\" has a Fixed constraint for {} but its dependencies push it past that date.",
                self.node_name, self.required_date
            ),
            ConstraintKind::Latest => format!(
                "\"{}\" has a Latest constraint of {} but its dependencies push it past that date.",
                self.node_name, self.required_date
            ),
            ConstraintKind::Earliest => format!(
                "\"{}\" has an Earliest constraint of {} that could not be met.",
                self.node_name, self.required_date
            ),
        }
    }
}
// }}}

// ── Serde helpers for HashMap<NodeId, V> (JSON requires string keys) ─────── {{{
mod nodeid_map_serde {
    use super::*;

    pub fn serialize<S, V>(map: &HashMap<NodeId, V>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        s.collect_map(map.iter().map(|(k, v)| (k.to_string(), v)))
    }

    pub fn deserialize<'de, D, V>(d: D) -> Result<HashMap<NodeId, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let string_map = HashMap::<String, V>::deserialize(d)?;
        string_map
            .into_iter()
            .map(|(k, v)| {
                NodeId::from_str(&k)
                    .map(|id| (id, v))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}
// }}}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NodeAllocations {
    pub tasks: HashMap<TaskId, TaskState>,
    pub milestones: HashMap<MilestoneId, MilestoneAllocation>,
    #[serde(with = "nodeid_map_serde")]
    pub constraint_violations: HashMap<NodeId, ConstraintViolation>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl NodeAllocations {
    /// Keep Fixed allocations (anchored tasks), clear Dynamic ones, milestones,
    /// and constraint violations.
    pub fn invalidate(&mut self) {
        // Keep Fixed allocations (anchored tasks: Complete, OnHold, etc.) and
        // any entry whose status is not NotStarted (e.g. InProgress tasks that
        // have Dynamic scheduler output — we must preserve their status so the
        // scheduler can recognise them in the next pass).
        self.tasks.retain(|_, state| {
            matches!(state.allocation, TaskAllocation::Fixed { .. })
                || state.status != Status::NotStarted
        });
        self.milestones.clear();
        self.constraint_violations.clear();
    }

    /// True if any Dynamic allocations or milestones exist (i.e. scheduler has run).
    pub fn has_schedule(&self) -> bool {
        self.tasks
            .values()
            .any(|ts| matches!(ts.allocation, TaskAllocation::Dynamic { .. }))
            || !self.milestones.is_empty()
    }
}
// }}}
