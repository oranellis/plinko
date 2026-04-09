use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::TagId;
use crate::data::ids::TaskId;
use crate::data::ids::UserId;
use crate::data::user::User;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerSlot {
    Specific {
        user_id: UserId,
        workload_days: f32,
    },
    Placeholder {
        required_tags: HashSet<TagId>,
        workload_days: f32,
    },
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl WorkerSlot {
    pub fn workload_days(&self) -> f32 {
        match self {
            WorkerSlot::Specific { workload_days, .. }
            | WorkerSlot::Placeholder { workload_days, .. } => *workload_days,
        }
    }

    pub fn is_satisfied_by(&self, user: &User) -> bool {
        match self {
            WorkerSlot::Specific { user_id, .. } => user.id == *user_id,
            WorkerSlot::Placeholder { required_tags, .. } => required_tags.is_subset(&user.tags),
        }
    }
}
// }}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    pub workers: Vec<WorkerSlot>,
    pub constraint: Option<DateConstraint>,
    pub duration_days_target: f32,
    /// When true the scheduler uses relaxed allocation, filling any available
    /// capacity each day. When false (default) the scheduler only schedules on
    /// days with enough capacity for the full daily block.
    #[serde(default)]
    pub relaxed_mode: bool,
    /// The date the task was actually started. Used as the scheduling origin
    /// for InProgress tasks so allocation is placed from this date forward.
    #[serde(default)]
    pub actual_start: Option<NaiveDate>,
    /// Optional context label shown alongside the task name (e.g. Monday group or parent item).
    #[serde(default)]
    pub context_label: Option<String>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Task {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            workers: Vec::new(),
            constraint: None,
            duration_days_target: 0.0,
            relaxed_mode: false,
            actual_start: None,
            context_label: None,
        }
    }

    pub fn with_equal_split(
        name: impl Into<String>,
        description: impl Into<String>,
        total_days: f32,
        users: &[UserId],
    ) -> Self {
        let per_user = if users.is_empty() {
            0.0
        } else {
            total_days / users.len() as f32
        };
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            workers: users
                .iter()
                .map(|&user_id| WorkerSlot::Specific {
                    user_id,
                    workload_days: per_user,
                })
                .collect(),
            constraint: None,
            duration_days_target: 0.0,
            relaxed_mode: false,
            actual_start: None,
            context_label: None,
        }
    }

    pub fn add_specific_worker(&mut self, user_id: UserId, workload_days: f32) {
        self.workers.push(WorkerSlot::Specific {
            user_id,
            workload_days,
        });
    }

    pub fn add_placeholder_worker(
        &mut self,
        required_tags: impl IntoIterator<Item = TagId>,
        workload_days: f32,
    ) {
        self.workers.push(WorkerSlot::Placeholder {
            required_tags: required_tags.into_iter().collect(),
            workload_days,
        });
    }

    pub fn total_workload_days(&self) -> f32 {
        self.workers.iter().map(WorkerSlot::workload_days).sum()
    }

    pub fn assigned_users(&self) -> impl Iterator<Item = UserId> + '_ {
        self.workers.iter().filter_map(|slot| match slot {
            WorkerSlot::Specific { user_id, .. } => Some(*user_id),
            WorkerSlot::Placeholder { .. } => None,
        })
    }

    pub fn with_duration(mut self, days: f32) -> Self {
        self.duration_days_target = days.max(0.0);
        self
    }

    pub fn effective_duration_days(&self) -> f32 {
        if self.duration_days_target > 0.0 {
            return self.duration_days_target;
        }
        let max = self
            .workers
            .iter()
            .map(WorkerSlot::workload_days)
            .fold(0.0_f32, f32::max);
        max * 2.0
    }
}
// }}}
