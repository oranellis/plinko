// modules{{{
use crate::{
    planner::plan::PlanSchedulerDataPatch,
    types::{
        crud::Crud,
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, PlanUserDataPatch},
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
    SchedulerData(PlanSchedulerDataPatch),
}

pub type TaskUpdate = Crud<NewTask, TaskPatch>;
pub type TaskGroupUpdate = Crud<NewTaskGroup, TaskGroupPatch>;
pub type PlanBufferUpdate = Crud<NewPlanBuffer, PlanBufferPatch>;
pub type MilestoneUpdate = Crud<NewMilestone, MilestonePatch>;
pub type UserUpdate = Crud<NewPlanUser, PlanUserDataPatch>;
pub type TagUpdate = Crud<NewTag, TagPatch>;
