// modules{{{
use uuid::Uuid;

use crate::{
    planner::plan::PlanSchedulerDataPatch,
    types::{
        milestone::{Milestone, MilestonePatch},
        tag::{Tag, TagPatch},
        task::{Task, TaskPatch},
        user::{PlanUserData, PlanUserDataPatch},
    },
};
// }}}

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
    Update(TaskPatch),
    Remove(Uuid),
}

pub enum MilestoneUpdate {
    Add(Milestone),
    Update(MilestonePatch),
    Remove(Uuid),
}

pub enum UserUpdate {
    Add(Uuid, PlanUserData),
    Update(PlanUserDataPatch),
    Remove(Uuid),
}

pub enum TagUpdate {
    Add(Tag),
    Update(TagPatch),
    Remove(Uuid),
}

pub enum SchedulerDataUpdate {
    Update(PlanSchedulerDataPatch),
}
