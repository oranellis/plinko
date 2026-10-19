// modules {{{
use uuid::Uuid;

use crate::{
    availability::rule::{NewRule, RulePatch},
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    planner::items::{
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        plan_delay::{NewPlanDelay, PlanDelayPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
    },
    planner::resources::{
        affinity::{AffinityPatch, NewAffinity},
        data_block::{DataBlockPatch, NewDataBlock},
        tag::{NewTag, TagPatch},
        target::{NewTarget, TargetPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
    },
};
// }}}

/// Update payloads that name the item they apply to
pub trait Identified {
    fn id(&self) -> Uuid;
    fn set_id(&mut self, id: Uuid);
}

// macros {{{
macro_rules! identified {
    ($($ty:ty),* $(,)?) => {
        $(impl Identified for $ty {
            fn id(&self) -> Uuid {
                self.id
            }
            fn set_id(&mut self, id: Uuid) {
                self.id = id;
            }
        })*
    };
}
// }}}

identified!(
    NewAffinity,
    AffinityPatch,
    NewDataBlock,
    DataBlockPatch,
    NewTask,
    TaskPatch,
    NewTaskGroup,
    TaskGroupPatch,
    NewMilestone,
    MilestonePatch,
    NewPlanBuffer,
    PlanBufferPatch,
    NewPlanDelay,
    PlanDelayPatch,
    NewTag,
    TagPatch,
    NewTarget,
    TargetPatch,
    NewPlanUser,
    PlanUserDataPatch,
    NewPlanUserGroup,
    PlanUserGroupPatch,
    NewTenantLoginUser,
    TenantLoginUserPatch,
    NewTenantUserGroup,
    TenantUserGroupPatch,
);

// impl NewRule {{{
impl<V> Identified for NewRule<V> {
    fn id(&self) -> Uuid {
        self.id
    }
    fn set_id(&mut self, id: Uuid) {
        self.id = id;
    }
}
// }}}

// impl RulePatch {{{
impl<V> Identified for RulePatch<V> {
    fn id(&self) -> Uuid {
        self.id
    }
    fn set_id(&mut self, id: Uuid) {
        self.id = id;
    }
}
// }}}
