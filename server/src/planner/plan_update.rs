// modules{{{
use serde::{Deserialize, Serialize};
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
        target::{NewTarget, TargetPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
        workload::Workload,
    },
};
// }}}

#[derive(Clone, Serialize, Deserialize)]
pub enum PlanUpdate {
    Task(Crud<NewTask, TaskPatch>),
    TaskGroup(Crud<NewTaskGroup, TaskGroupPatch>),
    Milestone(Crud<NewMilestone, MilestonePatch>),
    PlanBuffer(Crud<NewPlanBuffer, PlanBufferPatch>),
    User(Crud<NewPlanUser, PlanUserDataPatch>),
    UserGroup(Crud<NewPlanUserGroup, PlanUserGroupPatch>),
    Tag(Crud<NewTag, TagPatch>),
    Target(Crud<NewTarget, TargetPatch>),
    Affinity(Crud<NewAffinity, AffinityPatch>),
    AffinityRules(Uuid, RuleSetUpdate<bool>),
    /// Data blocks on a task or milestone
    Data(PlanNode, Crud<NewDataBlock, DataBlockPatch>),
    Availability(RuleSetUpdate<Workload>),
    SchedulerData(PlanSchedulerDataPatch),
}

impl PlanUpdate {
    /// Gives everything the update creates a fresh id, replacing whatever a client sent. Users and
    /// user groups added to a plan are existing tenant ones, so keep their ids.
    pub fn assign_ids(&mut self) {
        match self {
            PlanUpdate::Task(update) => update.assign_id(),
            PlanUpdate::TaskGroup(update) => update.assign_id(),
            PlanUpdate::Milestone(update) => update.assign_id(),
            PlanUpdate::PlanBuffer(update) => update.assign_id(),
            PlanUpdate::Tag(update) => update.assign_id(),
            PlanUpdate::Target(update) => update.assign_id(),
            PlanUpdate::Affinity(update) => update.assign_id(),
            PlanUpdate::AffinityRules(_, update) => update.assign_ids(),
            PlanUpdate::Data(_, update) => update.assign_id(),
            PlanUpdate::Availability(update) => update.assign_ids(),
            PlanUpdate::User(_) | PlanUpdate::UserGroup(_) | PlanUpdate::SchedulerData(_) => {}
        }
    }
}
