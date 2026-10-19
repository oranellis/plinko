// modules{{{
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::{calendar::Calendar, rule_set::RuleSet},
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{error::PlinkoResult, short_id::ShortId, workload::Workload},
};
// }}}

/// Admins see and edit everything, members create plans and edit the plans they are part of
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Member,
}

// Role supporting types {{{
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TenantLoginUser {
    id: Uuid,
    name: String,
    email: String,
    role: Role,
    pub(super) availability: RuleSet<Workload>,
    /// The plans the person has starred, which only they change
    #[serde(default)]
    pub(super) favourites: HashSet<ShortId>,
}

// impl TenantLoginUser{{{
impl From<NewTenantLoginUser> for TenantLoginUser {
    fn from(new: NewTenantLoginUser) -> Self {
        Self {
            id: new.id,
            name: new.name,
            email: new.email,
            role: new.role,
            availability: RuleSet::default(),
            favourites: HashSet::new(),
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
            availability: RuleSet::default(),
            favourites: HashSet::new(),
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

    pub fn availability(&self) -> &RuleSet<Workload> {
        &self.availability
    }

    pub fn favourites(&self) -> &HashSet<ShortId> {
        &self.favourites
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

/// Derived from a user's rules and rebuilt when they change, so it is never part of the user's
/// own state
#[derive(Clone, Debug, Default)]
pub struct TenantUserVolatileState {
    pub calendar: Option<Calendar<Workload>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewTenantLoginUser {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TenantLoginUserPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<Role>,
}
// }}}
