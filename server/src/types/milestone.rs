// modules{{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};
// }}}

#[derive(Debug, PartialEq)]
pub struct Milestone {
    id: Uuid,
    name: String,
    persistent_state: PersistentState,
}

// impl Milestone{{{
impl Milestone {
    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl From<NewMilestone> for Milestone {
    fn from(new: NewMilestone) -> Self {
        Self {
            id: new.id,
            name: new.name,
            persistent_state: PersistentState::new(),
        }
    }
}

impl Patchable<MilestonePatch> for Milestone {
    fn apply_patch(&mut self, patch: MilestonePatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(p) = patch.persistent_state {
            self.persistent_state = p
        }
        Ok(())
    }
}
// }}}

#[derive(Debug, PartialEq)]
pub struct PersistentState {
    actual_date: Option<NaiveDate>,
}

// impl PersistentState{{{
impl Default for PersistentState {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentState {
    pub fn new() -> Self {
        Self { actual_date: None }
    }
}
// }}}

#[derive(Debug, PartialEq)]
pub struct VolatileState {
    derived_status: Status,
    date: NaiveDate,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}

pub struct NewMilestone {
    pub id: Uuid,
    pub name: String,
}

pub struct MilestonePatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub persistent_state: Option<PersistentState>,
}
