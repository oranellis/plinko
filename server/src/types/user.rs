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

#[derive(Debug, PartialEq)]
pub struct PlanUserData {
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}

// impl User{{{
impl PlanUserData {
    pub fn new() -> Self {
        Self {
            weekly_schedules: WeeklySchedule::new_initial_schedule(),
            calendar_changes: CalendarChanges::new(),
            tags: Vec::new(),
        }
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

pub struct PlanUserDataPatch {
    pub id: Uuid,
    pub weekly_schedule: Option<WeeklySchedule>,
    pub calendar_changes: Option<CalendarChanges>,
    pub tags: Option<Vec<Tag>>,
}
