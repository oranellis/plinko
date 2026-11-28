// modules{{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::rule_set::RuleSetUpdate,
    availability::workload::Workload,
    common::crud::Crud,
    planner::items::{
        dependency::PlanNode,
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelayPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
    },
    planner::plan::PlanSchedulerDataPatch,
    planner::resources::{
        affinity::{AffinityPatch, NewAffinity},
        data_block::{DataBlockPatch, NewDataBlock},
        tag::{NewTag, TagPatch},
        target::{NewTarget, TargetPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
    },
};
// }}}

#[derive(Clone, Serialize, Deserialize)]
pub enum PlanUpdate {
    Task(Crud<NewTask, TaskPatch>),
    TaskGroup(Crud<NewTaskGroup, TaskGroupPatch>),
    Milestone(Crud<NewMilestone, MilestonePatch>),
    PlanBuffer(Crud<NewPlanBuffer, PlanBufferPatch>),
    PlanDelay(Crud<NewPlanDelay, PlanDelayPatch>),
    User(Crud<NewPlanUser, PlanUserDataPatch>),
    UserGroup(Crud<NewPlanUserGroup, PlanUserGroupPatch>),
    Tag(Crud<NewTag, TagPatch>),
    Target(Crud<NewTarget, TargetPatch>),
    Affinity(Crud<NewAffinity, AffinityPatch>),
    AffinityRules(Uuid, RuleSetUpdate<bool>),
    /// Data blocks on a task, milestone or task group
    Data(PlanNode, Crud<NewDataBlock, DataBlockPatch>),
    /// The description of a task, milestone or task group, `None` to clear it
    Describe(PlanNode, Option<String>),
    Availability(RuleSetUpdate<Workload>),
    SchedulerData(PlanSchedulerDataPatch),
}

// impl PlanUpdate {{{
impl PlanUpdate {
    pub fn set_dependencies(node: PlanNode, dependencies: Vec<PlanNode>) -> Self {
        Self::rewire(node, None, Some(dependencies))
    }

    pub fn rewire(
        node: PlanNode,
        parent: Option<Parent>,
        dependencies: Option<Vec<PlanNode>>,
    ) -> Self {
        match node {
            PlanNode::Task(id) => PlanUpdate::Task(Crud::Update(TaskPatch {
                id,
                name: None,
                parent,
                duration: None,
                time_constraint: None,
                dependencies,
                allocations: None,
                affinities: None,
                actual_workers: None,
                persistent_state: None,
            })),
            PlanNode::Milestone(id) => PlanUpdate::Milestone(Crud::Update(MilestonePatch {
                id,
                name: None,
                parent,
                time_constraint: None,
                dependencies,
                persistent_state: None,
            })),
            PlanNode::PlanBuffer(id) => PlanUpdate::PlanBuffer(Crud::Update(PlanBufferPatch {
                id,
                parent,
                dependencies,
                set_start_day: None,
                duration: None,
            })),
            PlanNode::PlanDelay(id) => PlanUpdate::PlanDelay(Crud::Update(PlanDelayPatch {
                id,
                name: None,
                parent,
                dependencies,
                duration: None,
            })),
            PlanNode::TaskGroup(id) => PlanUpdate::TaskGroup(Crud::Update(TaskGroupPatch {
                time_constraint: None,
                id,
                name: None,
                dependencies,
                parent,
            })),
        }
    }

    /// Only applies once nothing refers to the item, see `Plan::removal`
    pub fn remove(node: PlanNode) -> Self {
        match node {
            PlanNode::Task(id) => PlanUpdate::Task(Crud::Remove(id)),
            PlanNode::Milestone(id) => PlanUpdate::Milestone(Crud::Remove(id)),
            PlanNode::PlanBuffer(id) => PlanUpdate::PlanBuffer(Crud::Remove(id)),
            PlanNode::PlanDelay(id) => PlanUpdate::PlanDelay(Crud::Remove(id)),
            PlanNode::TaskGroup(id) => PlanUpdate::TaskGroup(Crud::Remove(id)),
        }
    }

    pub fn rewires(&self) -> Option<PlanNode> {
        match self {
            PlanUpdate::Task(Crud::Update(p)) if p.dependencies.is_some() || p.parent.is_some() => {
                Some(PlanNode::Task(p.id))
            }
            PlanUpdate::Milestone(Crud::Update(p))
                if p.dependencies.is_some() || p.parent.is_some() =>
            {
                Some(PlanNode::Milestone(p.id))
            }
            PlanUpdate::PlanBuffer(Crud::Update(p))
                if p.dependencies.is_some() || p.parent.is_some() =>
            {
                Some(PlanNode::PlanBuffer(p.id))
            }
            PlanUpdate::PlanDelay(Crud::Update(p))
                if p.dependencies.is_some() || p.parent.is_some() =>
            {
                Some(PlanNode::PlanDelay(p.id))
            }
            PlanUpdate::TaskGroup(Crud::Update(p))
                if p.dependencies.is_some() || p.parent.is_some() =>
            {
                Some(PlanNode::TaskGroup(p.id))
            }
            _ => None,
        }
    }

    /// Users and user groups keep their ids, as they are existing tenant ones
    pub fn assign_ids(&mut self) {
        match self {
            PlanUpdate::Task(update) => update.assign_id(),
            PlanUpdate::TaskGroup(update) => update.assign_id(),
            PlanUpdate::Milestone(update) => update.assign_id(),
            PlanUpdate::PlanBuffer(update) => update.assign_id(),
            PlanUpdate::PlanDelay(update) => update.assign_id(),
            PlanUpdate::Tag(update) => update.assign_id(),
            PlanUpdate::Target(update) => update.assign_id(),
            PlanUpdate::Affinity(update) => update.assign_id(),
            PlanUpdate::AffinityRules(_, update) => update.assign_ids(),
            PlanUpdate::Data(_, update) => update.assign_id(),
            PlanUpdate::Describe(..) => {}
            PlanUpdate::Availability(update) => update.assign_ids(),
            PlanUpdate::User(_) | PlanUpdate::UserGroup(_) | PlanUpdate::SchedulerData(_) => {}
        }
    }
}
// }}}
