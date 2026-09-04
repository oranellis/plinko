use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::types::{
    milestone::{self, Milestone},
    schedule::{CalendarChanges, WeeklySchedules},
    short_id::ShortId,
    tag::Tag,
    task::{self, Task},
    user::User,
};

#[derive(Debug, PartialEq)]
pub struct Plan {
    id: ShortId,
    tasks: HashMap<Uuid, Task>,
    milestones: HashMap<Uuid, Milestone>,
    users: HashMap<Uuid, User>,
    tags: HashMap<Uuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
    scheduler_data: PlanSchedulerData,
}

#[derive(Debug, PartialEq)]
struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: Option<Uuid>,
}

pub struct PlanVolatileState {
    tasks: HashMap<Uuid, task::VolatileState>,
    milestones: HashMap<Uuid, milestone::VolatileState>,
}
