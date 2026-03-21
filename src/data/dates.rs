use crate::data::ids::{MilestoneId, TaskId};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartDates {
    pub tasks: HashMap<TaskId, NaiveDate>,
    pub milestones: HashMap<MilestoneId, NaiveDate>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl StartDates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_task(&mut self, id: TaskId, date: NaiveDate) {
        self.tasks.insert(id, date);
    }

    pub fn set_milestone(&mut self, id: MilestoneId, date: NaiveDate) {
        self.milestones.insert(id, date);
    }

    pub fn task(&self, id: &TaskId) -> Option<NaiveDate> {
        self.tasks.get(id).copied()
    }

    pub fn milestone(&self, id: &MilestoneId) -> Option<NaiveDate> {
        self.milestones.get(id).copied()
    }
}
// }}}
