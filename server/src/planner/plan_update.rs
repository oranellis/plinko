use uuid::Uuid;

use crate::{
    planner::plan::PlanSchedulerData,
    types::{milestone::Milestone, tag::Tag, task::Task, user::User},
};

pub struct PlanUpdate {
    pub update_type: PlanUpdateType,
    pub time: chrono::DateTime<chrono::Utc>,
}

pub enum PlanUpdateType {
    Task(TaskUpdate),
    User(UserUpdate),
    Milestone(MilestoneUpdate),
    Tag(TagUpdate),
    SchedulerData(SchedulerDataUpdate),
}

pub enum TaskUpdate {
    Add(Task),
    Update(Task),
    Remove(Uuid),
}

pub enum MilestoneUpdate {
    Add(Milestone),
    Update(Milestone),
    Remove(Uuid),
}

pub enum UserUpdate {
    Add(User),
    Update(User),
    Remove(Uuid),
}

pub enum TagUpdate {
    Add(Tag),
    Update(Tag),
    Remove(Uuid),
}

pub enum SchedulerDataUpdate {
    Update(PlanSchedulerData),
}
