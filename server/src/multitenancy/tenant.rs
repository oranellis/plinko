// modules{{{
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{
    multitenancy::{
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{Role, TenantLoginUser},
        user_group::TenantUserGroup,
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
    traits::entity::Entity,
    types::{
        crud::Crud,
        error::{PlinkoError, PlinkoResult},
        short_id::ShortId,
        user::NewPlanUser,
    },
};
// }}}

pub struct Tenant {
    id: ShortId,
    name: String,
    pub(super) users: HashMap<Uuid, TenantLoginUser>,
    pub(super) user_groups: HashMap<Uuid, TenantUserGroup>,
    pub(super) plans: HashMap<ShortId, Plan>,
}

// impl Tenant{{{
impl Tenant {
    pub fn new(id: ShortId, name: String) -> Self {
        let mut users = HashMap::new();
        let root_user = TenantLoginUser::new(
            "Root User".to_string(),
            "root@localhost".to_string(),
            Role::Admin,
        );
        users.insert(*root_user.id(), root_user);
        Self {
            id,
            name,
            users,
            user_groups: HashMap::new(),
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
            user_groups: HashMap::new(),
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

    pub fn user_groups(&self) -> &HashMap<Uuid, TenantUserGroup> {
        &self.user_groups
    }

    pub fn plans(&self) -> &HashMap<ShortId, Plan> {
        &self.plans
    }

    fn insert_plan(&mut self, plan: Plan) -> PlinkoResult<()> {
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
        let is_admin = self
            .users
            .get(&u.actor)
            .ok_or(PlinkoError::Forbidden(u.actor))?
            .role()
            == Role::Admin;
        match u.update_type {
            TenantUpdateType::User(update) => {
                let own_profile =
                    matches!(&update, Crud::Update(p) if p.id == u.actor && p.role.is_none());
                allow(is_admin || own_profile, u.actor)?;
                TenantLoginUser::apply_crud(self, update)
            }
            TenantUpdateType::UserGroup(update) => {
                allow(is_admin, u.actor)?;
                TenantUserGroup::apply_crud(self, update)
            }
            TenantUpdateType::Plan(update) => self.plan_update(u.actor, is_admin, update),
        }
    }

    fn plan_update(
        &mut self,
        actor: Uuid,
        is_admin: bool,
        update: PlanTenantUpdate,
    ) -> PlinkoResult<()> {
        match update {
            PlanTenantUpdate::Add(new) => {
                let mut plan = Plan::from(new);
                plan.apply_update(PlanUpdate::User(Crud::Add(NewPlanUser { id: actor })))?;
                self.insert_plan(plan)
            }
            PlanTenantUpdate::Remove(id) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.plans
                    .remove(&id)
                    .map(drop)
                    .ok_or(PlinkoError::PlanDoesNotExist(id))
            }
            PlanTenantUpdate::Update(id, update) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.ensure_plan_refs_exist(&update)?;
                self.plans
                    .get_mut(&id)
                    .ok_or(PlinkoError::PlanDoesNotExist(id))?
                    .apply_update(update)
            }
        }
    }

    /// Admins and anyone in the plan, directly or through an assigned group, may change a plan
    fn authorize_plan(&self, actor: Uuid, is_admin: bool, id: &ShortId) -> PlinkoResult<()> {
        let plan = self
            .plans
            .get(id)
            .ok_or(PlinkoError::PlanDoesNotExist(*id))?;
        let in_plan = plan.users().contains_key(&actor)
            || plan
                .user_groups()
                .keys()
                .any(|g| self.group_members(*g).contains(&actor));
        allow(is_admin || in_plan, actor)
    }

    /// Plans only know ids, so users and groups added to one must exist in the tenant
    fn ensure_plan_refs_exist(&self, update: &PlanUpdate) -> PlinkoResult<()> {
        let (id, exists) = match update {
            PlanUpdate::User(Crud::Add(new)) => (new.id, self.users.contains_key(&new.id)),
            PlanUpdate::UserGroup(Crud::Add(new)) => {
                (new.id, self.user_groups.contains_key(&new.id))
            }
            _ => return Ok(()),
        };
        exists
            .then_some(())
            .ok_or(PlinkoError::ItemDoesNotExist(id))
    }

    /// Everyone in the group or any group below it
    pub fn group_members(&self, group: Uuid) -> HashSet<Uuid> {
        let mut members = HashSet::new();
        let mut pending = vec![group];
        while let Some(id) = pending.pop() {
            if let Some(group) = self.user_groups.get(&id) {
                members.extend(group.users());
                let children = self
                    .user_groups
                    .iter()
                    .filter(|(_, g)| g.parent() == Some(id));
                pending.extend(children.map(|(child, _)| *child));
            }
        }
        members
    }
}
// }}}

fn allow(permitted: bool, actor: Uuid) -> PlinkoResult<()> {
    permitted.then_some(()).ok_or(PlinkoError::Forbidden(actor))
}
