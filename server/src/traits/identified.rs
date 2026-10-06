use uuid::Uuid;

use crate::{
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    types::{
        milestone::{MilestonePatch, NewMilestone},
        plan_buffer::{NewPlanBuffer, PlanBufferPatch},
        tag::{NewTag, TagPatch},
        task::{NewTask, TaskPatch},
        task_group::{NewTaskGroup, TaskGroupPatch},
        user::{NewPlanUser, PlanUserDataPatch},
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
    NewTenantLoginUser,
    TenantLoginUserPatch,
    NewTenantUserGroup,
    TenantUserGroupPatch,
);
