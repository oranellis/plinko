use std::collections::HashMap;

use chrono::NaiveDate;

use crate::types::workload::Workload;

#[derive(Debug, PartialEq)]
pub struct CalendarChanges {
    changes: HashMap<NaiveDate, Workload>,
}

impl CalendarChanges {
    pub fn new() -> Self {
        Self {
            changes: HashMap::new(),
        }
    }

    pub fn insert_if_novel(&mut self, date: NaiveDate, workload: Workload) -> bool {
        self.changes.insert(date, workload).is_some()
    }

    pub fn remove(&mut self, date: &NaiveDate) -> bool {
        self.changes.remove(date).is_some()
    }
}

#[derive(Debug, PartialEq)]
pub struct WeeklySchedule {
    pub monday: Workload,
    pub tuesday: Workload,
    pub wednesday: Workload,
    pub thursday: Workload,
    pub friday: Workload,
    pub saturday: Workload,
    pub sunday: Workload,
    pub start_date: NaiveDate,
}

#[derive(Debug, PartialEq)]
pub struct WeeklySchedules {
    schedules: HashMap<NaiveDate, WeeklySchedule>,
}

impl WeeklySchedules {
    pub fn new(initial_schedule: WeeklySchedule) -> Self {
        let mut schedules = WeeklySchedules {
            schedules: HashMap::new(),
        };
        schedules.schedules.insert(NaiveDate::MIN, initial_schedule);
        schedules
    }

    pub fn insert(&mut self, schedule: WeeklySchedule) {
        self.schedules.insert(schedule.start_date, schedule);
    }

    pub fn remove(&mut self, date: &NaiveDate) {
        self.schedules.remove(date);
    }
}
