// modules{{{
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    multitenancy::{
        user::{TenantLoginUser, TenantLoginUserPatch},
        user_group::{TenantUserGroup, TenantUserGroupPatch},
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
    types::short_id::ShortId,
};
// }}}

/// The deterministic boundary for mutating a tenant. Every change to tenant state is a
/// `TenantUpdate`, and `time` is the only source of "now" for anything it touches.
pub struct TenantUpdate {
    pub time: DateTime<Utc>,
    pub update_type: TenantUpdateType,
}

pub enum TenantUpdateType {
    User(UserUpdate),
    UserGroup(UserGroupUpdate),
    Plan(PlanTenantUpdate),
}

pub enum UserUpdate {
    Add(TenantLoginUser),
    Update(TenantLoginUserPatch),
    Remove(Uuid),
}

pub enum UserGroupUpdate {
    Add(TenantUserGroup),
    Update(TenantUserGroupPatch),
    Remove(Uuid),
}

pub enum PlanTenantUpdate {
    Add(Box<Plan>),
    Remove(ShortId),
    /// Mutate the contents of an existing plan
    Update(ShortId, PlanUpdate),
}
