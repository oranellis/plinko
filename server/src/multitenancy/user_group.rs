use std::collections::HashSet;

// modules {{{
use uuid::Uuid;

use crate::traits::patch::Patchable;

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

impl Patchable<TenantUserGroupPatch> for TenantUserGroup {
    fn apply_patch(
        &mut self,
        patch: TenantUserGroupPatch,
    ) -> crate::types::error::PlinkoResult<()> {
        todo!()
    }
}
// }}}

pub struct TenantUserGroupPatch {
    id: Uuid,
    name: Option<String>,
    users: Option<HashSet<Uuid>>,
}
