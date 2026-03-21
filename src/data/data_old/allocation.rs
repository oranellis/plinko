//! The result of a scheduler run, storing day-by-day task assignments.

use crate::data::constraint::ConstraintKind;
use crate::data::ids::{MilestoneId, NodeId, TaskId, UserId};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single contiguous block of work performed by one user on one day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSegment {
    pub date: NaiveDate,
    /// Hours actually worked on this day (may be partial due to holidays or
    /// competing tasks consuming part of the day's capacity).
    pub hours_worked: f32,
}

/// All work done by one user for a single worker slot on a task.
/// Segments are ordered chronologically; gaps between dates indicate
/// non-working days or days the user had no remaining capacity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotAllocation {
    pub user_id: UserId,
    pub segments: Vec<WorkSegment>,
}

/// Complete allocation record for one task, covering all of its worker slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAllocation {
    pub task_id: TaskId,
    /// One entry per worker slot, in the same order as `Task::workers`.
    pub slot_allocations: Vec<SlotAllocation>,
    /// Earliest date across all slot first-segments.
    pub start_date: NaiveDate,
    /// Latest date across all slot last-segments.
    pub end_date: NaiveDate,
}

/// The date on which a milestone is scheduled to occur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneAllocation {
    pub milestone_id: MilestoneId,
    pub date: NaiveDate,
}

/// Records that a task or milestone could not meet its scheduled constraint
/// and was pushed to the earliest possible date instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintViolation {
    /// Display name of the task or milestone.
    pub node_name: String,
    /// The kind of constraint that was violated.
    pub kind: ConstraintKind,
    /// The date the constraint required.
    pub required_date: NaiveDate,
    /// The date the scheduler actually placed it (earliest possible).
    pub scheduled_date: NaiveDate,
}

impl ConstraintViolation {
    /// A human-readable description of the violation.
    pub fn message(&self) -> String {
        match self.kind {
            ConstraintKind::Fixed => format!(
                "\"{}\" has a Fixed constraint requiring {}, but the earliest possible start is {}.",
                self.node_name, self.required_date, self.scheduled_date
            ),
            ConstraintKind::Latest => format!(
                "\"{}\" has a Latest constraint of {}, but the earliest possible start is {}.",
                self.node_name, self.required_date, self.scheduled_date
            ),
            ConstraintKind::Earliest => format!(
                "\"{}\" has an Earliest constraint of {} that could not be met.",
                self.node_name, self.required_date
            ),
        }
    }
}

/// The complete output of one scheduler run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAllocation {
    pub tasks: HashMap<TaskId, TaskAllocation>,
    pub milestones: HashMap<MilestoneId, MilestoneAllocation>,
    /// Tasks or milestones whose scheduling constraint could not be met.
    /// The scheduler pushes them to the earliest possible date and records the violation here.
    pub constraint_violations: HashMap<NodeId, ConstraintViolation>,
}

impl PlanAllocation {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            constraint_violations: HashMap::new(),
        }
    }
}

impl Default for PlanAllocation {
    fn default() -> Self {
        Self::new()
    }
}
