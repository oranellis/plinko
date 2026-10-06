use uuid::Uuid;

use crate::{
    availability::rule::{NewRule, RulePatch},
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    types::{
        affinity::{AffinityPatch, NewAffinity},
        data_block::{DataBlockPatch, NewDataBlock},
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch, PlanUserGroupPatch},
    },
};

/// Update payloads that name the item they apply to
pub trait Identified {
    fn id(&self) -> Uuid;
}

macro_rules! identified {
    ($($ty:ty),* $(,)?) => {
        $(impl Identified for $ty {
            fn id(&self) -> Uuid {
                self.id
            }
        })*
    };
}

identified!(
    NewAffinity,
    AffinityPatch,
    NewDataBlock,
    DataBlockPatch,
    NewRule,
    RulePatch,
    NewTask,
    TaskPatch,
    NewTaskGroup,
    TaskGroupPatch,
    NewMilestone,
    MilestonePatch,
    NewPlanBuffer,
    PlanBufferPatch,
    NewTag,
    TagPatch,
    NewPlanUser,
    PlanUserDataPatch,
    NewPlanUserGroup,
    PlanUserGroupPatch,
    NewTenantLoginUser,
    TenantLoginUserPatch,
    NewTenantUserGroup,
    TenantUserGroupPatch,
);
