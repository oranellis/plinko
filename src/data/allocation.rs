use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::data::{MilestoneId, NodeId, TaskId, UserId, WorkerSlot};

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
    status: Status,
    pub(crate) allocation: TaskAllocation,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneAllocation {
    date: NaiveDate,
    derived_status: Status,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NodeAllocations {
    pub tasks: HashMap<TaskId, TaskState>,
    pub milestones: HashMap<MilestoneId, MilestoneAllocation>,
}

impl NodeAllocations {
    pub fn from_old_allocations(old_allocations: &Self) -> Self {
        let tasks = old_allocations
            .tasks
            .iter()
            .filter(|(_, state)| matches!(state.allocation, TaskAllocation::Fixed { .. }))
            .map(|(id, state)| (*id, state.clone()))
            .collect();
        Self {
            tasks,
            milestones: HashMap::new(),
        }
    }
}
