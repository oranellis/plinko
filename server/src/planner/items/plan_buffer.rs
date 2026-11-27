// modules {{{
use crate::helpers::double_option;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{dates::DateRange, error::PlinkoResult},
    helpers::functions::check_id_match,
    planner::items::{dependency::PlanNode, parent::Parent},
    traits::patch::Patchable,
};
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanBuffer {
    id: Uuid,
    parent: Parent,
    dependencies: Vec<PlanNode>,
    set_start_day: Option<NaiveDate>,
    duration: u32,
}

// impl PlanBuffer {{{
impl PlanBuffer {
    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn dependencies(&self) -> &Vec<PlanNode> {
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
            self.dependencies = PlanNode::merged(p);
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

// PlanBuffer supporting types {{{
/// Written by the scheduler for every buffer it could schedule
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewPlanBuffer {
    pub id: Uuid,
    pub parent: Parent,
    pub duration: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanBufferPatch {
    pub id: Uuid,
    pub parent: Option<Parent>,
    pub dependencies: Option<Vec<PlanNode>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub set_start_day: Option<Option<NaiveDate>>,
    pub duration: Option<u32>,
}
// }}}
