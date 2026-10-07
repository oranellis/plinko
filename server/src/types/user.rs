// modules{{{
use std::collections::HashSet;

use uuid::Uuid;

use crate::{traits::patch::Patchable, types::error::PlinkoResult};
// }}}

/// A tenant user's place in a plan, with the plan's tags they hold
#[derive(Clone, Debug, Default, PartialEq)]
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

/// A tenant user group assigned to a plan, so anyone in it can be allocated tasks
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanUserGroupData;

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

pub struct NewPlanUserGroup {
    pub id: Uuid,
}

pub struct PlanUserGroupPatch {
    pub id: Uuid,
}

pub struct NewPlanUser {
    pub id: Uuid,
}

pub struct PlanUserDataPatch {
    pub id: Uuid,
    pub tags: Option<HashSet<Uuid>>,
}
