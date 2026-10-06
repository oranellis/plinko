// modules{{{
use uuid::Uuid;

use crate::{
    availability::rule_set::RuleSetUpdate,
    planner::plan::PlanSchedulerDataPatch,
    types::{
        affinity::{AffinityPatch, NewAffinity},
        crud::Crud,
        data_block::{DataBlockPatch, NewDataBlock},
        dependency::PlanNode,
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
        workload::Workload,
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
    Affinity(Crud<NewAffinity, AffinityPatch>),
    AffinityRules(Uuid, RuleSetUpdate<bool>),
    /// Data blocks on a task or milestone
    Data(PlanNode, Crud<NewDataBlock, DataBlockPatch>),
    Availability(RuleSetUpdate<Workload>),
    SchedulerData(PlanSchedulerDataPatch),
}
