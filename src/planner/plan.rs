use std::collections::HashMap;

use chrono::NaiveDate;
use nanoid::nanoid;
use uuid::Uuid;

use crate::types::{
    milestone::{self, Milestone, MilestoneUuid},
    schedule::{CalendarChanges, WeeklySchedules},
    tag::{Tag, TagUuid},
    task::{self, Task, TaskUuid},
    user::{User, UserUuid},
};

pub struct Plan {
    id: ShortId,
    tasks: HashMap<TaskUuid, Task>,
    milestones: HashMap<MilestoneUuid, Milestone>,
    users: HashMap<UserUuid, User>,
    tags: HashMap<TagUuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
    scheduler_data: PlanSchedulerData,
}

impl Plan {}

struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: Option<TaskUuid>,
}

pub struct PlanVolatileState {
    tasks: HashMap<Uuid, task::VolatileState>,
    milestones: HashMap<Uuid, milestone::VolatileState>,
}

#[derive(Default)]
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
