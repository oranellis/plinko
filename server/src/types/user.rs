use uuid::Uuid;

use crate::types::{
    schedule::{CalendarChanges, WeeklySchedule},
    tag::Tag,
};

#[derive(Debug, PartialEq)]
pub struct User {
    id: Uuid,
    name: String,
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}

impl User {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            weekly_schedules: WeeklySchedule::new_initial_schedule(),
            calendar_changes: CalendarChanges::new(),
            tags: Vec::new(),
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}
