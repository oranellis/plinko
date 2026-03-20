use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::data::{
    CalendarOverrides, Milestone, MilestoneId, NodeId, Task, TaskId, User, UserId, WorkSchedule,
    allocation::NodeAllocations, tag::Tag, user::UserData,
};

pub struct Plan {
    pub id: Uuid,
    pub name: String,
    pub users_data: HashMap<UserId, UserData>,
    pub tags: Vec<Tag>,
    pub tasks: HashMap<TaskId, Task>,
    pub milestones: HashMap<MilestoneId, Milestone>,
    pub plan_start_date: NaiveDate,
    pub default_schedule: WorkSchedule,
    pub calendar_overrides: CalendarOverrides,
    pub scheduler_target: NodeId,
    pub node_allocations: NodeAllocations,
}

impl Plan {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            users_data: HashMap::new(),
            tags: Vec::new(),
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            plan_start_date: chrono::Local::now().date_naive(),
            default_schedule: WorkSchedule::default(),
            calendar_overrides: CalendarOverrides::default(),
            scheduler_target: NodeId::PlanStart,
            node_allocations: NodeAllocations::default(),
        }
    }

    // ———————— User data ————————

    pub fn schedule_for(&self, user_id: &UserId) -> &WorkSchedule {
        self.users_data
            .get(user_id)
            .and_then(|user| user.schedule.as_ref())
            .unwrap_or(&self.default_schedule)
    }

    pub fn set_user_schedule(&mut self, user_id: UserId, schedule: Option<WorkSchedule>) {
        if let Some(user) = self.users_data.get_mut(&user_id) {
            user.schedule = schedule;
        }
    }

    pub fn hours_available(&self, user_id: &UserId, date: NaiveDate) -> f32 {
        if let Some(schedule) = self.users_data.get(user_id).and_then(|u| u.schedule) {
            schedule.hours_on()
        }
    }
}
