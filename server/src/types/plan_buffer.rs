// modules {{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{common::DateRange, dependency::Dependency, error::PlinkoResult},
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanBuffer {
    id: Uuid,
    parent: Option<Uuid>,
    dependencies: Vec<Dependency>,
    set_start_day: Option<NaiveDate>,
    duration: u32,
}

// impl PlanBuffer {{{
impl PlanBuffer {
    pub fn parent(&self) -> Option<Uuid> {
        self.parent
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn set_start_day(&self) -> &Option<NaiveDate> {
        &self.set_start_day
    }

    pub fn duration(&self) -> &u32 {
        &self.duration
    }
}

impl From<NewPlanBuffer> for PlanBuffer {
    fn from(new: NewPlanBuffer) -> Self {
        Self {
            id: new.id,
            parent: new.parent,
            dependencies: Vec::new(),
            set_start_day: None,
            duration: new.duration,
        }
    }
}

impl Patchable<PlanBufferPatch> for PlanBuffer {
    fn apply_patch(&mut self, patch: PlanBufferPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(p) = patch.dependencies {
            self.dependencies = p;
        }
        if let Some(p) = patch.set_start_day {
            self.set_start_day = p
        }
        if let Some(p) = patch.duration {
            self.duration = p;
        }
        Ok(())
    }
}
// }}}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VolatileState {
    pub date_range: Option<DateRange>,
}

pub struct NewPlanBuffer {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub duration: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanBufferPatch {
    pub id: Uuid,
    pub parent: Option<Option<Uuid>>,
    pub dependencies: Option<Vec<Dependency>>,
    pub set_start_day: Option<Option<NaiveDate>>,
    pub duration: Option<u32>,
}
