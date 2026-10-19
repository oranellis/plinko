// modules{{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    multitenancy::{
        tenant::Tenant,
        user::{NewTenantLoginUser, TenantLoginUser, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroup, TenantUserGroupPatch},
    },
    traits::entity::Entity,
    types::{
        error::{PlinkoError, PlinkoResult},
        task::Assignee,
    },
};
// }}}

// impl Entity for TenantLoginUser {{{
impl Entity<Tenant> for TenantLoginUser {
    type New = NewTenantLoginUser;
    type Patch = TenantLoginUserPatch;

    fn entities(tenant: &Tenant) -> &HashMap<Uuid, Self> {
        &tenant.users
    }
    fn entities_mut(tenant: &mut Tenant) -> &mut HashMap<Uuid, Self> {
        &mut tenant.users
    }
    /// The tenant always keeps an admin, so a role change cannot demote the last one
    fn validate(tenant: &Tenant, _id: Uuid) -> PlinkoResult<()> {
        match tenant.admins().next() {
            Some(_) => Ok(()),
            None => Err(PlinkoError::LastAdmin),
        }
    }

    fn in_use(tenant: &Tenant, id: Uuid) -> bool {
        let last_admin = tenant.admins().eq([&id]);
        let recorded = tenant
            .plans
            .values()
            .any(|p| p.recorded_in_use(Assignee::User(id)));
        last_admin
            || recorded
            || tenant.plans.values().any(|p| p.users().contains_key(&id))
            || tenant.user_groups.values().any(|g| g.users().contains(&id))
    }
}
// }}}

// impl Entity for TenantUserGroup {{{
impl Entity<Tenant> for TenantUserGroup {
    type New = NewTenantUserGroup;
    type Patch = TenantUserGroupPatch;

    fn entities(tenant: &Tenant) -> &HashMap<Uuid, Self> {
        &tenant.user_groups
    }
    fn entities_mut(tenant: &mut Tenant) -> &mut HashMap<Uuid, Self> {
        &mut tenant.user_groups
    }

    /// Members must be tenant users and the parent chain must exist and not loop back
    fn validate(tenant: &Tenant, id: Uuid) -> PlinkoResult<()> {
        let group = &tenant.user_groups[&id];
        if let Some(missing) = group.users().iter().find(|u| !tenant.users.contains_key(u)) {
            return Err(PlinkoError::ItemDoesNotExist(*missing));
        }
        let mut next = group.parent();
        while let Some(parent) = next {
            if parent == id {
                return Err(PlinkoError::GroupCycle(id));
            }
            next = tenant
                .user_groups
                .get(&parent)
                .ok_or(PlinkoError::ItemDoesNotExist(parent))?
                .parent();
        }
        Ok(())
    }

    fn in_use(tenant: &Tenant, id: Uuid) -> bool {
        tenant
            .plans
            .values()
            .any(|p| p.user_groups().contains_key(&id))
            || tenant.user_groups.values().any(|g| g.parent() == Some(id))
    }
}
// }}}
