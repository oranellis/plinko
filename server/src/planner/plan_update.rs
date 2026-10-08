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
        dependency::{Dependency, PlanNode},
        milestone::{MilestonePatch, NewMilestone},
        parent::Parent,
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
    /// Replaces the dependencies of an item and changes nothing else
    pub fn set_dependencies(node: PlanNode, dependencies: Vec<Dependency>) -> Self {
        Self::rewire(node, None, Some(dependencies))
    }

    /// Changes the group and the dependencies of an item, whichever are given, and nothing else
    pub fn rewire(
        node: PlanNode,
        parent: Option<Parent>,
        dependencies: Option<Vec<Dependency>>,
    ) -> Self {
        match node {
            PlanNode::Task(id) => PlanUpdate::Task(Crud::Update(TaskPatch {
                id,
                name: None,
                parent,
                mode: None,
                duration: None,
                time_constraint: None,
                dependencies,
                allocations: None,
                affinities: None,
                actual_workers: None,
                description: None,
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
            PlanNode::TaskGroup(id) => PlanUpdate::TaskGroup(Crud::Update(TaskGroupPatch {
                id,
                name: None,
                dependencies,
                parent,
                placeholder_duration: None,
            })),
        }
    }

    /// Takes the item out of the plan, once nothing refers to it
    pub fn remove(node: PlanNode) -> Self {
        match node {
            PlanNode::Task(id) => PlanUpdate::Task(Crud::Remove(id)),
            PlanNode::Milestone(id) => PlanUpdate::Milestone(Crud::Remove(id)),
            PlanNode::PlanBuffer(id) => PlanUpdate::PlanBuffer(Crud::Remove(id)),
            PlanNode::TaskGroup(id) => PlanUpdate::TaskGroup(Crud::Remove(id)),
        }
    }

    /// The item whose dependencies or place in a group the update changes, if it does
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
            PlanUpdate::TaskGroup(Crud::Update(p))
                if p.dependencies.is_some() || p.parent.is_some() =>
            {
                Some(PlanNode::TaskGroup(p.id))
            }
            _ => None,
        }
    }

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
