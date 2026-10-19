// modules{{{
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{dates::TimeConstraint, error::PlinkoResult},
    helpers::functions::check_id_match,
    planner::items::{dependency::Dependency, parent::Parent},
    planner::resources::data_block::DataBlocks,
    traits::patch::Patchable,
};
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Milestone {
    id: Uuid,
    name: String,
    parent: Parent,
    time_constraint: TimeConstraint,
    dependencies: Vec<Dependency>,
    description: Option<String>,
    data: DataBlocks,
    persistent_state: PersistentState,
}

// impl Milestone{{{
impl Milestone {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn persistent_state(&self) -> &PersistentState {
        &self.persistent_state
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub(crate) fn set_description(&mut self, description: Option<String>) {
        self.description = description;
    }

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
            description: None,
            data: DataBlocks::default(),
            persistent_state: PersistentState::default(),
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
            self.dependencies = Dependency::merged(d);
        }
        if let Some(p) = patch.persistent_state {
            self.persistent_state = p
        }
        Ok(())
    }
}
// }}}

// Milestone supporting types {{{
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistentState {
    pub actual_date: Option<NaiveDate>,
}

/// Written by the scheduler for every milestone it could schedule
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub derived_status: DerivedStatus,
    pub date: NaiveDate,
}

/// Milestones have no status of their own, it comes from what they depend on
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivedStatus {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewMilestone {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MilestonePatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub parent: Option<Parent>,
    pub time_constraint: Option<TimeConstraint>,
    pub dependencies: Option<Vec<Dependency>>,
    pub persistent_state: Option<PersistentState>,
}
// }}}

// impl VolatileState {{{
impl VolatileState {
    /// The exclusive end of the one day the milestone lands on, so dependents treat it like any
    /// other node (see `Dependency::start_after`)
    pub fn end(&self) -> NaiveDate {
        self.date + chrono::Days::new(1)
    }
}
// }}}
