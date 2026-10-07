use crate::helpers::double_option;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// modules {{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};

// }}}

/// A team of users. A group's parent makes teams hierarchical, so anyone in a subgroup is also
/// in every group above it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TenantUserGroup {
    id: Uuid,
    name: String,
    parent: Option<Uuid>,
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

    pub fn parent(&self) -> Option<Uuid> {
        self.parent
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
            parent: new.parent,
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
        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(u) = patch.users {
            self.users = u;
        }
        Ok(())
    }
}
// }}}

#[derive(Serialize, Deserialize)]
pub struct NewTenantUserGroup {
    pub id: Uuid,
    pub name: String,
    pub parent: Option<Uuid>,
    pub users: HashSet<Uuid>,
}

#[derive(Serialize, Deserialize)]
pub struct TenantUserGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub parent: Option<Option<Uuid>>,
    pub users: Option<HashSet<Uuid>>,
}
