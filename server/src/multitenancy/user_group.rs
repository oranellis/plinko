use std::collections::HashSet;

// modules {{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};

// }}}

pub struct TenantUserGroup {
    id: Uuid,
    name: String,
    users: HashSet<Uuid>,
}

// impl TenantUserGroup {{{
impl TenantUserGroup {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn users(&self) -> &HashSet<Uuid> {
        &self.users
    }
}

impl From<NewTenantUserGroup> for TenantUserGroup {
    fn from(new: NewTenantUserGroup) -> Self {
        Self {
            id: new.id,
            name: new.name,
            users: new.users,
        }
    }
}

impl Patchable<TenantUserGroupPatch> for TenantUserGroup {
    fn apply_patch(&mut self, patch: TenantUserGroupPatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(u) = patch.users {
            self.users = u;
        }
        Ok(())
    }
}
// }}}

pub struct NewTenantUserGroup {
    pub id: Uuid,
    pub name: String,
    pub users: HashSet<Uuid>,
}

pub struct TenantUserGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub users: Option<HashSet<Uuid>>,
}
