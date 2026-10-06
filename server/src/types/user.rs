// modules{{{
use uuid::Uuid;

use crate::{
    traits::patch::Patchable,
    types::{
        error::PlinkoResult,
        schedule::{CalendarChanges, WeeklySchedule},
        tag::Tag,
    },
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanUserData {
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}

// impl User{{{
impl Default for PlanUserData {
    fn default() -> Self {
        Self {
            weekly_schedules: WeeklySchedule::new_initial_schedule(),
            calendar_changes: CalendarChanges::new(),
            tags: Vec::new(),
        }
    }
}

impl From<NewPlanUser> for PlanUserData {
    fn from(_: NewPlanUser) -> Self {
        Self::default()
    }
}

impl Patchable<PlanUserDataPatch> for PlanUserData {
    fn apply_patch(&mut self, patch: PlanUserDataPatch) -> PlinkoResult<()> {
        if let Some(w) = patch.weekly_schedule {
            self.weekly_schedules = w
        }
        if let Some(c) = patch.calendar_changes {
            self.calendar_changes = c
        }
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
    pub weekly_schedule: Option<WeeklySchedule>,
    pub calendar_changes: Option<CalendarChanges>,
    pub tags: Option<Vec<Tag>>,
}
