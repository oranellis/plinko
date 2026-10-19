// modules{{{
use crate::{
    availability::rule_set::RuleSetUpdate,
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    types::{crud::Crud, short_id::ShortId, workload::Workload},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
// }}}

#[derive(Clone, Serialize, Deserialize)]
/// The deterministic boundary for mutating a tenant. Every change to tenant state is a
/// `TenantUpdate`, and `time` is the only source of "now" for anything it touches.
pub struct TenantUpdate {
    pub time: DateTime<Utc>,
    /// The login user making the change, who must be allowed to
    pub actor: Uuid,
    pub update_type: TenantUpdateType,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TenantUpdateType {
    User(Crud<NewTenantLoginUser, TenantLoginUserPatch>),
    UserGroup(Crud<NewTenantUserGroup, TenantUserGroupPatch>),
    /// Change one user's availability, which only they or an admin may do
    Availability(Uuid, RuleSetUpdate<Workload>),
    /// Moves the tenant's clock, which only admins (the server runner) may do
    SetToday(NaiveDate),
    /// Stars a plan for the person making the update, or takes the star off
    Favourite(ShortId, bool),
    Plan(PlanTenantUpdate),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum PlanTenantUpdate {
    Add(NewPlan),
    Remove(ShortId),
    /// Mutate the contents of an existing plan
    Update(ShortId, Box<PlanUpdate>),
}

impl TenantUpdateType {
    /// Gives everything the update creates a fresh id, replacing whatever a client sent. A server
    /// handler calls this before building the `TenantUpdate` it applies and logs.
    pub fn assign_ids(&mut self) {
        match self {
            TenantUpdateType::User(update) => update.assign_id(),
            TenantUpdateType::UserGroup(update) => update.assign_id(),
            TenantUpdateType::Availability(_, update) => update.assign_ids(),
            TenantUpdateType::SetToday(_) | TenantUpdateType::Favourite(..) => {}
            TenantUpdateType::Plan(update) => update.assign_ids(),
        }
    }
}

impl PlanTenantUpdate {
    fn assign_ids(&mut self) {
        match self {
            PlanTenantUpdate::Add(new) => new.id = ShortId::new(),
            PlanTenantUpdate::Update(_, update) => update.assign_ids(),
            PlanTenantUpdate::Remove(_) => {}
        }
    }
}
