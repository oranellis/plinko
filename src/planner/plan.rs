use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::types::{
    milestone::{self, Milestone, MilestoneUuid},
    schedule::{CalendarChanges, WeeklySchedules},
    tag::{Tag, TagUuid},
    task::{self, Task, TaskUuid},
    user::{User, UserUuid},
};

struct Plan {
    tasks: HashMap<TaskUuid, Task>,
    milestones: HashMap<MilestoneUuid, Milestone>,
    users: HashMap<UserUuid, User>,
    tags: HashMap<TagUuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
    scheduler_data: PlanSchedulerData,
}

struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: Option<TaskUuid>,
}

struct PlanVolatileState {
    tasks: HashMap<Uuid, task::VolatileState>,
    milestones: HashMap<Uuid, milestone::VolatileState>,
}
