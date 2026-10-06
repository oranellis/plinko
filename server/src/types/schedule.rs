// modules{{{
use std::collections::HashMap;

use chrono::NaiveDate;

use crate::types::{error::PlinkoResult, workload::Workload};
// }}}

#[derive(Debug, PartialEq)]
pub struct CalendarChanges {
    changes: HashMap<NaiveDate, Workload>,
}

// impl CalendarChanges{{{
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

impl Default for CalendarChanges {
    fn default() -> Self {
        Self::new()
    }
}
// }}}

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

// impl WeeklySchedule{{{
impl WeeklySchedule {
    pub fn new_initial_schedule() -> Self {
        WeeklySchedule {
            monday: Workload::new_unchecked(8),
            tuesday: Workload::new_unchecked(8),
            wednesday: Workload::new_unchecked(8),
            thursday: Workload::new_unchecked(8),
            friday: Workload::new_unchecked(8),
            saturday: Workload::new_unchecked(0),
            sunday: Workload::new_unchecked(0),
            start_date: NaiveDate::MIN,
        }
    }

    /// Hours per day, Monday first.
    pub fn new_from_days(hours: [u8; 7], start_date: NaiveDate) -> PlinkoResult<Self> {
        let [
            monday,
            tuesday,
            wednesday,
            thursday,
            friday,
            saturday,
            sunday,
        ] = hours.map(Workload::new);
        Ok(Self {
            monday: monday?,
            tuesday: tuesday?,
            wednesday: wednesday?,
            thursday: thursday?,
            friday: friday?,
            saturday: saturday?,
            sunday: sunday?,
            start_date,
        })
    }
}
// }}}

#[derive(Debug, PartialEq)]
pub struct WeeklySchedules {
    schedules: HashMap<NaiveDate, WeeklySchedule>,
}

// impl WeeklySchedules{{{
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
// }}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_from_days_validates_hours() {
        let d = NaiveDate::MIN;
        let s = WeeklySchedule::new_from_days([8, 8, 8, 8, 4, 0, 0], d).unwrap();
        assert_eq!(s.friday.get(), 4);
        assert!(WeeklySchedule::new_from_days([8, 8, 8, 8, 9, 0, 0], d).is_err());
    }
}
