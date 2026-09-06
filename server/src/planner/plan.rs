use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::types::{
    dependency::PlanNode,
    milestone::{self, Milestone},
    schedule::{CalendarChanges, WeeklySchedule, WeeklySchedules},
    short_id::ShortId,
    tag::Tag,
    task::{self, Task},
    user::User,
};

#[derive(Debug, PartialEq)]
pub struct Plan {
    id: ShortId,
    name: String,
    tasks: HashMap<Uuid, Task>,
    milestones: HashMap<Uuid, Milestone>,
    users: HashMap<Uuid, User>,
    tags: HashMap<Uuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
    scheduler_data: PlanSchedulerData,
}

impl Plan {
    pub fn id(&self) -> &ShortId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(id: ShortId, name: String, start_date: NaiveDate) -> Self {
        Self {
            id,
            name,
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            users: HashMap::new(),
            tags: HashMap::new(),
            plan_weekly_schedule: WeeklySchedules::new(WeeklySchedule::new_initial_schedule()),
            plan_calendar_changes: CalendarChanges::new(),
            scheduler_data: PlanSchedulerData::new(start_date),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: PlanNode,
}

impl PlanSchedulerData {
    fn new(start_date: NaiveDate) -> Self {
        Self {
            start_date,
            scheduler_target: PlanNode::PlanStart,
        }
    }

    pub fn set_start_date(&mut self, date: NaiveDate) {
        self.start_date = date;
    }

    pub fn set_scheduler_target(&mut self, node: PlanNode) {
        self.scheduler_target = node;
    }

    pub fn start_date(&self) -> &NaiveDate {
        &self.start_date
    }

    pub fn scheduler_target(&self) -> &PlanNode {
        &self.scheduler_target
    }
}

#[derive(Debug, PartialEq)]
pub struct PlanVolatileState {
    pub tasks: HashMap<Uuid, task::VolatileState>,
    pub milestones: HashMap<Uuid, milestone::VolatileState>,
}

impl PlanVolatileState {
    pub fn new(
        tasks: HashMap<Uuid, task::VolatileState>,
        milestones: HashMap<Uuid, milestone::VolatileState>,
    ) -> Self {
        Self { tasks, milestones }
    }
}
