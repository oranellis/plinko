use std::collections::HashMap;

use uuid::Uuid;

use crate::data::{Milestone, MilestoneId, Task, TaskId, User, UserId, WorkSchedule};

pub struct UserData {
    user: User,
    schedule: WorkSchedule,
}

impl UserData {
    pub fn new(user: User) -> Self {
        Self {
            user,
            schedule: WorkSchedule::weekdays(),
        }
    }

    pub fn with_schedule(mut self, work_schedule: WorkSchedule) -> UserData {
        self.schedule = work_schedule;
        self
    }

    pub fn user_mut(&mut self) -> &mut User {
        &mut self.user
    }

    pub fn schedule_mut(&mut self) -> &mut WorkSchedule {
        &mut self.schedule
    }
}

pub struct Plan {
    pub id: Uuid,
    pub name: String,
    pub users_data: HashMap<UserId, UserData>,
    pub tasks: HashMap<TaskId, Task>,
    pub milestones: HashMap<MilestoneId, Milestone>,
}
