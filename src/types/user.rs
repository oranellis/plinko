use uuid::Uuid;

use crate::types::{
    schedule::{CalendarChanges, WeeklySchedule},
    tag::Tag,
};

pub struct User {
    id: UserUuid,
    name: String,
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}

pub type UserUuid = Uuid;
