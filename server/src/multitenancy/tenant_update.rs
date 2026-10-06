// modules{{{
use crate::{
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    types::{crud::Crud, short_id::ShortId},
};
use chrono::{DateTime, Utc};
// }}}

/// The deterministic boundary for mutating a tenant. Every change to tenant state is a
/// `TenantUpdate`, and `time` is the only source of "now" for anything it touches.
pub struct TenantUpdate {
    pub time: DateTime<Utc>,
    pub update_type: TenantUpdateType,
}

pub enum TenantUpdateType {
    User(Crud<NewTenantLoginUser, TenantLoginUserPatch>),
    UserGroup(Crud<NewTenantUserGroup, TenantUserGroupPatch>),
    Plan(PlanTenantUpdate),
}

pub enum PlanTenantUpdate {
    Add(NewPlan),
    Remove(ShortId),
    /// Mutate the contents of an existing plan
    Update(ShortId, PlanUpdate),
}
