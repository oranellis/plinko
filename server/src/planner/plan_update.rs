// modules{{{
use crate::{
    availability::rule_set::RuleSetUpdate,
    planner::plan::PlanSchedulerDataPatch,
    types::{
        crud::Crud,
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
    },
};
// }}}

pub enum PlanUpdate {
    Task(Crud<NewTask, TaskPatch>),
    TaskGroup(Crud<NewTaskGroup, TaskGroupPatch>),
    Milestone(Crud<NewMilestone, MilestonePatch>),
    PlanBuffer(Crud<NewPlanBuffer, PlanBufferPatch>),
    User(Crud<NewPlanUser, PlanUserDataPatch>),
    UserGroup(Crud<NewPlanUserGroup, PlanUserGroupPatch>),
    Tag(Crud<NewTag, TagPatch>),
    Availability(RuleSetUpdate),
    SchedulerData(PlanSchedulerDataPatch),
}
