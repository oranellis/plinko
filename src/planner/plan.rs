use std::collections::HashMap;

use chrono::NaiveDate;
use nanoid::nanoid;
use uuid::Uuid;

use crate::types::{
    milestone::{self, Milestone},
    schedule::{CalendarChanges, WeeklySchedules},
    tag::Tag,
    task::{self, Task},
    user::User,
};

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

struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: Option<Uuid>,
}

pub struct PlanVolatileState {
    tasks: HashMap<Uuid, task::VolatileState>,
    milestones: HashMap<Uuid, milestone::VolatileState>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct ShortId([u8; 8]);

impl ShortId {
    fn new() -> Self {
        let id = nanoid!(8);
        let mut buf = [0u8; 8];
        buf.copy_from_slice(id.as_bytes());
        Self(buf)
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("nanoid output is always valid ASCII/UTF-8")
    }
}

impl std::fmt::Display for ShortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
