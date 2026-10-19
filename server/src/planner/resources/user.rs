// modules{{{
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use uuid::Uuid;

use crate::{common::error::PlinkoResult, traits::patch::Patchable};
// }}}

/// A tenant user's place in a plan, with the plan's tags they hold
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanUserData {
    tags: HashSet<Uuid>,
}

// impl User{{{
impl PlanUserData {
    pub fn tags(&self) -> &HashSet<Uuid> {
        &self.tags
    }
}

impl From<NewPlanUser> for PlanUserData {
    fn from(_: NewPlanUser) -> Self {
        Self::default()
    }
}

impl Patchable<PlanUserDataPatch> for PlanUserData {
    fn apply_patch(&mut self, patch: PlanUserDataPatch) -> PlinkoResult<()> {
        if let Some(t) = patch.tags {
            self.tags = t
        }
        Ok(())
    }
}
// }}}

// PlanUserData supporting types {{{
/// A tenant user group assigned to a plan, so anyone in it can be allocated tasks
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanUserGroupData;

#[derive(Clone, Serialize, Deserialize)]
pub struct NewPlanUserGroup {
    pub id: Uuid,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlanUserGroupPatch {
    pub id: Uuid,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewPlanUser {
    pub id: Uuid,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlanUserDataPatch {
    pub id: Uuid,
    pub tags: Option<HashSet<Uuid>>,
}
// }}}

// impl PlanUserGroupData {{{
impl From<NewPlanUserGroup> for PlanUserGroupData {
    fn from(_: NewPlanUserGroup) -> Self {
        Self
    }
}

impl Patchable<PlanUserGroupPatch> for PlanUserGroupData {
    fn apply_patch(&mut self, _patch: PlanUserGroupPatch) -> PlinkoResult<()> {
        Ok(())
    }
}
// }}}
