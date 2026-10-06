// modules{{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    multitenancy::{
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::TenantLoginUser,
    },
    planner::plan::Plan,
    types::{
        error::{PlinkoError, PlinkoResult},
        short_id::ShortId,
    },
};
// }}}

pub struct Tenant {
    id: ShortId,
    name: String,
    users: HashMap<Uuid, TenantLoginUser>,
    plans: HashMap<ShortId, Plan>,
}

// impl Tenant{{{
impl Tenant {
    pub fn new(id: ShortId, name: String) -> Self {
        let mut users = HashMap::new();
        let root_user = TenantLoginUser::new("Root User".to_string(), "root@localhost".to_string());
        users.insert(*root_user.id(), root_user);
        Self {
            id,
            name,
            users,
            plans: HashMap::new(),
        }
    }

    /// Create a new tenant with a specific initial login
    ///
    /// * `id`: A unique ID for the tenant
    /// * `name`: A friendly name for the tenant
    /// * `user`: An initial user to populate the tenant with
    pub fn new_with_login(id: ShortId, name: String, user: TenantLoginUser) -> Self {
        let mut users = HashMap::new();
        users.insert(*user.id(), user);
        Self {
            id,
            name,
            users,
            plans: HashMap::new(),
        }
    }

    pub fn id(&self) -> &ShortId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn users(&self) -> &HashMap<Uuid, TenantLoginUser> {
        &self.users
    }

    pub fn plans(&self) -> &HashMap<ShortId, Plan> {
        &self.plans
    }

    pub fn insert_plan(&mut self, plan: Plan) -> PlinkoResult<()> {
        match self.plans.entry(*plan.id()) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                Err(PlinkoError::PlanExists(*occupied_entry.get().id()))
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(plan);
                Ok(())
            }
        }
    }

    pub fn apply_update(&mut self, u: TenantUpdate) -> PlinkoResult<()> {
        match u.update_type {
            TenantUpdateType::Plan(update) => self.plan_update(update),
            TenantUpdateType::User(_) => todo!(),
            TenantUpdateType::UserGroup(_) => todo!(),
        }
    }

    fn plan_update(&mut self, update: PlanTenantUpdate) -> PlinkoResult<()> {
        match update {
            PlanTenantUpdate::Add(plan) => self.insert_plan(*plan),
            PlanTenantUpdate::Remove(id) => self
                .plans
                .remove(&id)
                .map(drop)
                .ok_or(PlinkoError::PlanDoesNotExist(id)),
            PlanTenantUpdate::Update(id, update) => self
                .plans
                .get_mut(&id)
                .ok_or(PlinkoError::PlanDoesNotExist(id))?
                .apply_update(update),
        }
    }
}
// }}}
