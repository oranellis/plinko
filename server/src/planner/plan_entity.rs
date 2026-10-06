// modules{{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    planner::plan::Plan,
    traits::entity::Entity,
    types::{
        affinity::{Affinity, AffinityPatch, NewAffinity},
        dependency::{Dependency, PlanNode},
        error::PlinkoResult,
        milestone::{Milestone, MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBuffer, PlanBufferPatch},
        tag::{NewTag, Tag, TagPatch},
        task::{NewTask, Task, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroup, TaskGroupPatch},
        user::{
            NewPlanUser, NewPlanUserGroup, PlanUserData, PlanUserDataPatch, PlanUserGroupData,
            PlanUserGroupPatch,
        },
    },
};
// }}}

/// A schedulable entity with a parent and dependencies, and so a place in the dependency graph
pub trait PlanItem: Entity<Plan> {
    fn node(id: Uuid) -> PlanNode;
    fn parent(&self) -> Parent;
    fn dependencies(&self) -> &[Dependency];
    fn workers(&self) -> Vec<Worker> {
        Vec::new()
    }
    fn affinities(&self) -> Vec<Uuid> {
        Vec::new()
    }
}

macro_rules! plan_item {
    ($ty:ident, $node:ident, $new:ty, $patch:ty, $field:ident
     $(, workers: $workers:expr, affinities: $affinities:expr)?) => {
        impl Entity<Plan> for $ty {
            type New = $new;
            type Patch = $patch;

            fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
                &plan.$field
            }
            fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
                &mut plan.$field
            }
            fn validate(plan: &Plan, id: Uuid) -> PlinkoResult<()> {
                plan.validate_item::<Self>(id)
            }
            fn in_use(plan: &Plan, id: Uuid) -> bool {
                plan.node_in_use(PlanNode::$node(id))
            }
        }

        impl PlanItem for $ty {
            fn node(id: Uuid) -> PlanNode {
                PlanNode::$node(id)
            }
            fn parent(&self) -> Parent {
                $ty::parent(self)
            }
            fn dependencies(&self) -> &[Dependency] {
                $ty::dependencies(self)
            }
            $(fn workers(&self) -> Vec<Worker> {
                $workers(self)
            }
            fn affinities(&self) -> Vec<Uuid> {
                $affinities(self)
            })?
        }
    };
}

plan_item!(Task, Task, NewTask, TaskPatch, tasks,
    workers: |t: &Task| t.allocations().keys().copied().collect(),
    affinities: |t: &Task| t.affinities().iter().copied().collect());
plan_item!(
    Milestone,
    Milestone,
    NewMilestone,
    MilestonePatch,
    milestones
);
plan_item!(
    PlanBuffer,
    PlanBuffer,
    NewPlanBuffer,
    PlanBufferPatch,
    plan_buffers
);
plan_item!(
    TaskGroup,
    TaskGroup,
    NewTaskGroup,
    TaskGroupPatch,
    task_groups
);

impl Entity<Plan> for Tag {
    type New = NewTag;
    type Patch = TagPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.tags
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.tags
    }
    fn in_use(plan: &Plan, id: Uuid) -> bool {
        plan.worker_in_use(Worker::Tag(id))
    }
}

impl Entity<Plan> for PlanUserData {
    type New = NewPlanUser;
    type Patch = PlanUserDataPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.users
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.users
    }
    fn in_use(plan: &Plan, id: Uuid) -> bool {
        plan.worker_in_use(Worker::User(id))
    }
}

impl Entity<Plan> for PlanUserGroupData {
    type New = NewPlanUserGroup;
    type Patch = PlanUserGroupPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.user_groups
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.user_groups
    }
    fn in_use(plan: &Plan, id: Uuid) -> bool {
        plan.worker_in_use(Worker::UserGroup(id))
    }
}

impl Entity<Plan> for Affinity {
    type New = NewAffinity;
    type Patch = AffinityPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.affinities
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.affinities
    }
    fn in_use(plan: &Plan, id: Uuid) -> bool {
        plan.tasks.values().any(|t| t.affinities().contains(&id))
    }
}
