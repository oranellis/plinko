// modules{{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};
// }}}

/// Admins see and edit everything, members create plans and edit the plans they are part of
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Member,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TenantLoginUser {
    id: Uuid,
    name: String,
    email: String,
    role: Role,
}

// impl TenantLoginUser{{{
impl From<NewTenantLoginUser> for TenantLoginUser {
    fn from(new: NewTenantLoginUser) -> Self {
        Self {
            id: new.id,
            name: new.name,
            email: new.email,
            role: new.role,
        }
    }
}

impl TenantLoginUser {
    pub fn new(name: String, email: String, role: Role) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            email,
            role,
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn role(&self) -> Role {
        self.role
    }
}

impl Patchable<TenantLoginUserPatch> for TenantLoginUser {
    fn apply_patch(&mut self, patch: TenantLoginUserPatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(e) = patch.email {
            self.email = e;
        }
        if let Some(r) = patch.role {
            self.role = r;
        }
        Ok(())
    }
}
// }}}

pub struct NewTenantLoginUser {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
}

pub struct TenantLoginUserPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<Role>,
}
