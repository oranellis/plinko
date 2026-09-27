// modules{{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        error::PlinkoResult,
        schedule::{CalendarChanges, WeeklySchedule},
        tag::Tag,
    },
};
// }}}

#[derive(Debug, PartialEq)]
pub struct User {
    id: Uuid,
    name: String,
    weekly_schedules: WeeklySchedule,
    calendar_changes: CalendarChanges,
    tags: Vec<Tag>,
}

// impl User{{{
impl User {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            weekly_schedules: WeeklySchedule::new_initial_schedule(),
            calendar_changes: CalendarChanges::new(),
            tags: Vec::new(),
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl Patchable<UserPatch> for User {
    fn apply_patch(&mut self, patch: UserPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
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

pub struct UserPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub weekly_schedule: Option<WeeklySchedule>,
    pub calendar_changes: Option<CalendarChanges>,
    pub tags: Option<Vec<Tag>>,
}
