// modules{{{
use crate::{
    availability::rule_set::RuleSetUpdate,
    availability::workload::Workload,
    common::{crud::Crud, short_id::ShortId},
    multitenancy::{
        user::{NewTenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    planner::{plan::NewPlan, plan_update::PlanUpdate},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
// }}}

#[derive(Clone, Serialize, Deserialize)]
/// Every change to tenant state is one of these, and `time` is the only source of "now" for
/// anything it touches, so replaying is deterministic
pub struct TenantUpdate {
    pub time: DateTime<Utc>,
    pub actor: Uuid,
    pub update_type: TenantUpdateType,
}

// TenantUpdate supporting types {{{
#[derive(Clone, Serialize, Deserialize)]
pub enum TenantUpdateType {
    User(Crud<NewTenantLoginUser, TenantLoginUserPatch>),
    UserGroup(Crud<NewTenantUserGroup, TenantUserGroupPatch>),
    Availability(Uuid, RuleSetUpdate<Workload>),
    /// The availability new users and new plans start with
    DefaultSchedule(RuleSetUpdate<Workload>),
    /// Sent by the server runner each day
    SetToday(NaiveDate),
    /// Stars or unstars a plan for the actor
    Favourite(ShortId, bool),
    Plan(PlanTenantUpdate),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum PlanTenantUpdate {
    Add(NewPlan),
    Remove(ShortId),
    Update(ShortId, Box<PlanUpdate>),
}
// }}}

// impl TenantUpdateType {{{
impl TenantUpdateType {
    /// Replaces whatever ids a client sent for new items
    pub fn assign_ids(&mut self) {
        match self {
            TenantUpdateType::User(update) => update.assign_id(),
            TenantUpdateType::UserGroup(update) => update.assign_id(),
            TenantUpdateType::Availability(_, update)
            | TenantUpdateType::DefaultSchedule(update) => update.assign_ids(),
            TenantUpdateType::SetToday(_) | TenantUpdateType::Favourite(..) => {}
            TenantUpdateType::Plan(update) => update.assign_ids(),
        }
    }
}
// }}}

// impl PlanTenantUpdate {{{
impl PlanTenantUpdate {
    fn assign_ids(&mut self) {
        match self {
            PlanTenantUpdate::Add(new) => new.id = ShortId::new(),
            PlanTenantUpdate::Update(_, update) => update.assign_ids(),
            PlanTenantUpdate::Remove(_) => {}
        }
    }
}
// }}}
