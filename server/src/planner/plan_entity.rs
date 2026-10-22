// modules{{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    common::error::{PlinkoError, PlinkoResult},
    planner::items::{
        dependency::{Dependency, PlanNode, PlanNodeType},
        milestone::{Milestone, MilestonePatch, NewMilestone},
        parent::Parent,
        plan_buffer::{NewPlanBuffer, PlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelay, PlanDelayPatch},
        task::{Assignee, NewTask, Task, TaskPatch, Worker},
        task_group::{NewTaskGroup, TaskGroup, TaskGroupPatch},
    },
    planner::plan::Plan,
    planner::resources::{
        affinity::{Affinity, AffinityPatch, NewAffinity},
        tag::{NewTag, Tag, TagPatch},
        target::{NewTarget, Target, TargetPatch},
        user::{
            NewPlanUser, NewPlanUserGroup, PlanUserData, PlanUserDataPatch, PlanUserGroupData,
            PlanUserGroupPatch,
        },
    },
    traits::entity::Entity,
};
// }}}

/// What every node of a plan says about itself, which `Plan::node` gives for any id
pub trait NodeData {
    fn id(&self) -> Uuid;
    fn parent(&self) -> Parent;
    fn dependencies(&self) -> &[Dependency];
    fn node_type(&self) -> PlanNodeType;
}

/// A schedulable entity with a parent and dependencies, and so a place in the dependency graph
pub trait PlanItem: Entity<Plan> + NodeData {
    fn node(id: Uuid) -> PlanNode;
    fn workers(&self) -> Vec<Worker> {
        Vec::new()
    }
    fn affinities(&self) -> Vec<Uuid> {
        Vec::new()
    }
    /// The workers a person has been recorded for, which must be ones the item asks for
    fn recorded_workers(&self) -> Vec<Worker> {
        Vec::new()
    }
}

// plan_item {{{
macro_rules! plan_item {
    ($ty:ident, $node:ident, $new:ty, $patch:ty, $field:ident
     $(, workers: $workers:expr, affinities: $affinities:expr, recorded: $recorded:expr)?) => {
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

        impl NodeData for $ty {
            fn id(&self) -> Uuid {
                *$ty::id(self)
            }
            fn parent(&self) -> Parent {
                $ty::parent(self)
            }
            fn dependencies(&self) -> &[Dependency] {
                $ty::dependencies(self)
            }
            fn node_type(&self) -> PlanNodeType {
                PlanNodeType::$node
            }
        }

        impl PlanItem for $ty {
            fn node(id: Uuid) -> PlanNode {
                PlanNode::$node(id)
            }
            $(fn workers(&self) -> Vec<Worker> {
                $workers(self)
            }
            fn affinities(&self) -> Vec<Uuid> {
                $affinities(self)
            }
            fn recorded_workers(&self) -> Vec<Worker> {
                $recorded(self)
            })?
        }
    };
}

plan_item!(Task, Task, NewTask, TaskPatch, tasks,
    workers: |t: &Task| t.allocations().keys().copied().collect(),
    affinities: |t: &Task| t.affinities().iter().copied().collect(),
    recorded: |t: &Task| t.actual_workers().keys().copied().collect());
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
    PlanDelay,
    PlanDelay,
    NewPlanDelay,
    PlanDelayPatch,
    plan_delays
);
plan_item!(
    TaskGroup,
    TaskGroup,
    NewTaskGroup,
    TaskGroupPatch,
    task_groups
);
// }}}

// impl Entity for Tag {{{
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
        plan.worker_in_use(Worker::Tag(id)) || plan.users.values().any(|u| u.tags().contains(&id))
    }
}
// }}}

// impl Entity for PlanUserData {{{
impl Entity<Plan> for PlanUserData {
    type New = NewPlanUser;
    type Patch = PlanUserDataPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.users
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.users
    }
    fn validate(plan: &Plan, id: Uuid) -> PlinkoResult<()> {
        match plan.users[&id]
            .tags()
            .iter()
            .find(|t| !plan.tags.contains_key(t))
        {
            Some(missing) => Err(PlinkoError::ItemDoesNotExist(*missing)),
            None => Ok(()),
        }
    }
    fn in_use(plan: &Plan, id: Uuid) -> bool {
        plan.worker_in_use(Worker::User(id)) || plan.recorded_in_use(Assignee::User(id))
    }
}
// }}}

// impl Entity for PlanUserGroupData {{{
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
// }}}

// impl Entity for Affinity {{{
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
// }}}

// impl Entity for Target {{{
impl Entity<Plan> for Target {
    type New = NewTarget;
    type Patch = TargetPatch;

    fn entities(plan: &Plan) -> &HashMap<Uuid, Self> {
        &plan.targets
    }
    fn entities_mut(plan: &mut Plan) -> &mut HashMap<Uuid, Self> {
        &mut plan.targets
    }
    fn validate(plan: &Plan, id: Uuid) -> PlinkoResult<()> {
        plan.ensure_node_exists(&plan.targets[&id].node())
    }
}
// }}}

// impl Plan {{{
impl Plan {
    /// The node with the id, of whichever kind it is
    pub fn node(&self, id: &Uuid) -> Option<&dyn NodeData> {
        let node: Option<&dyn NodeData> = self
            .tasks
            .get(id)
            .map(|n| n as &dyn NodeData)
            .or_else(|| self.task_groups.get(id).map(|n| n as &dyn NodeData))
            .or_else(|| self.milestones.get(id).map(|n| n as &dyn NodeData))
            .or_else(|| self.plan_buffers.get(id).map(|n| n as &dyn NodeData))
            .or_else(|| self.plan_delays.get(id).map(|n| n as &dyn NodeData));
        node
    }
}
// }}}
