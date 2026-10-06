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
    Task(Crud<NewTask, TaskPatch>),
    TaskGroup(Crud<NewTaskGroup, TaskGroupPatch>),
    Milestone(Crud<NewMilestone, MilestonePatch>),
    PlanBuffer(Crud<NewPlanBuffer, PlanBufferPatch>),
    User(Crud<NewPlanUser, PlanUserDataPatch>),
    Tag(Crud<NewTag, TagPatch>),
    SchedulerData(PlanSchedulerDataPatch),
}
