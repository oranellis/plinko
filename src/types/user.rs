use uuid::Uuid;

use crate::types::{
    schedule::{CalendarChanges, WeeklySchedule},
    tag::Tag,
};

pub struct User {
    id: Uuid,
    name: String,
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}
