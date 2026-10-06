// modules{{{
use uuid::Uuid;

use crate::{
    planner::plan::PlanSchedulerDataPatch,
    types::{
        milestone::{MilestonePatch, NewMilestone},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        user::PlanUserDataPatch,
    },
};
// }}}

pub enum PlanUpdate {
    Task(TaskUpdate),
    User(UserUpdate),
    Milestone(MilestoneUpdate),
    Tag(TagUpdate),
    SchedulerData(SchedulerDataUpdate),
}

pub enum TaskUpdate {
    Add(NewTask),
    Update(TaskPatch),
    Remove(Uuid),
}

pub enum MilestoneUpdate {
    Add(NewMilestone),
    Update(MilestonePatch),
    Remove(Uuid),
}

pub enum UserUpdate {
    Add(Uuid),
    Update(PlanUserDataPatch),
    Remove(Uuid),
}

pub enum TagUpdate {
    Add(NewTag),
    Update(TagPatch),
    Remove(Uuid),
}

pub enum SchedulerDataUpdate {
    Update(PlanSchedulerDataPatch),
}
