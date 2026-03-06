use std::collections::HashMap;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use crate::data::ids::{MilestoneId, TaskId};

/// Tracks the start date of every task and milestone in a plan.
/// Kept separate from task/milestone definitions so dates can be recomputed
/// (e.g. when dependencies or workload change) without mutating the task data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartDates {
    pub tasks: HashMap<TaskId, NaiveDate>,
    pub milestones: HashMap<MilestoneId, NaiveDate>,
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ids::{MilestoneId, TaskId};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn new_is_empty() {
        let s = StartDates::new();
        assert!(s.tasks.is_empty());
        assert!(s.milestones.is_empty());
    }

    #[test]
    fn set_and_get_task() {
        let mut s = StartDates::new();
        let id = TaskId::new();
        s.set_task(id, date(2026, 3, 9));
        assert_eq!(s.task(&id), Some(date(2026, 3, 9)));
    }

    #[test]
    fn task_absent_is_none() {
        let s = StartDates::new();
        assert_eq!(s.task(&TaskId::new()), None);
    }

    #[test]
    fn set_and_get_milestone() {
        let mut s = StartDates::new();
        let id = MilestoneId::new();
        s.set_milestone(id, date(2026, 6, 1));
        assert_eq!(s.milestone(&id), Some(date(2026, 6, 1)));
    }

    #[test]
    fn milestone_absent_is_none() {
        let s = StartDates::new();
        assert_eq!(s.milestone(&MilestoneId::new()), None);
    }

    #[test]
    fn overwriting_a_date_updates_value() {
        let mut s = StartDates::new();
        let id = TaskId::new();
        s.set_task(id, date(2026, 3, 9));
        s.set_task(id, date(2026, 4, 1));
        assert_eq!(s.task(&id), Some(date(2026, 4, 1)));
    }
}
