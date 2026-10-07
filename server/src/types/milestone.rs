// modules{{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        common::TimeConstraint, data_block::DataBlocks, dependency::Dependency,
        error::PlinkoResult, parent::Parent,
    },
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct Milestone {
    id: Uuid,
    name: String,
    parent: Parent,
    time_constraint: TimeConstraint,
    dependencies: Vec<Dependency>,
    data: DataBlocks,
    persistent_state: PersistentState,
}

// impl Milestone{{{
impl Milestone {
    pub fn data(&self) -> &DataBlocks {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut DataBlocks {
        &mut self.data
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn time_constraint(&self) -> TimeConstraint {
        self.time_constraint
    }

    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl From<NewMilestone> for Milestone {
    fn from(new: NewMilestone) -> Self {
        Self {
            id: new.id,
            name: new.name,
            parent: new.parent,
            time_constraint: TimeConstraint::None,
            dependencies: Vec::new(),
            data: DataBlocks::default(),
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
        if let Some(p) = patch.parent {
            self.parent = p
        }
        if let Some(t) = patch.time_constraint {
            self.time_constraint = t;
        }
        if let Some(d) = patch.dependencies {
            self.dependencies = d;
        }
        if let Some(p) = patch.persistent_state {
            self.persistent_state = p
        }
        Ok(())
    }
}
// }}}

#[derive(Clone, Debug, PartialEq)]
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

/// Written by the scheduler for every milestone it could schedule
#[derive(Clone, Debug, PartialEq)]
pub struct VolatileState {
    pub derived_status: DerivedStatus,
    pub date: NaiveDate,
}

/// Milestones have no status of their own, it comes from what they depend on
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DerivedStatus {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}

pub struct NewMilestone {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

pub struct MilestonePatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub parent: Option<Parent>,
    pub time_constraint: Option<TimeConstraint>,
    pub dependencies: Option<Vec<Dependency>>,
    pub persistent_state: Option<PersistentState>,
}
