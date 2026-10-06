// modules{{{
use uuid::Uuid;

use crate::{
    planner::plan::PlanSchedulerDataPatch,
    types::{
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::PlanUserDataPatch,
    },
};
// }}}

pub enum PlanUpdate {
    Task(TaskUpdate),
    TaskGroup(TaskGroupUpdate),
    User(UserUpdate),
    Milestone(MilestoneUpdate),
    PlanBuffer(PlanBufferUpdate),
    Tag(TagUpdate),
    SchedulerData(SchedulerDataUpdate),
}

pub enum TaskUpdate {
    Add(NewTask),
    Update(TaskPatch),
    Remove(Uuid),
}

pub enum TaskGroupUpdate {
    Add(NewTaskGroup),
    Update(TaskGroupPatch),
    Remove(Uuid),
}

pub enum PlanBufferUpdate {
    Add(NewPlanBuffer),
    Update(PlanBufferPatch),
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
