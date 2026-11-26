// modules {{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::helpers::double_option;
use crate::{
    common::{dates::DateRange, error::PlinkoResult},
    helpers::functions::check_id_match,
    planner::items::{dependency::PlanNode, parent::Parent},
    traits::patch::Patchable,
};
// }}}

/// A wait of `duration` calendar days that begins when what it depends on ends. Like a task with
/// no work and no time constraint, the schedule never moves it for anything but its dependencies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanDelay {
    id: Uuid,
    name: Option<String>,
    parent: Parent,
    dependencies: Vec<PlanNode>,
    duration: u32,
}

// impl PlanDelay {{{
impl PlanDelay {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn dependencies(&self) -> &Vec<PlanNode> {
        &self.dependencies
    }

    pub fn duration(&self) -> &u32 {
        &self.duration
    }
}

impl From<NewPlanDelay> for PlanDelay {
    fn from(new: NewPlanDelay) -> Self {
        Self {
            id: new.id,
            name: new.name.filter(|n| !n.is_empty()),
            parent: new.parent,
            dependencies: Vec::new(),
            duration: new.duration,
        }
    }
}

impl Patchable<PlanDelayPatch> for PlanDelay {
    fn apply_patch(&mut self, patch: PlanDelayPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(n) = patch.name {
            self.name = n.filter(|n| !n.is_empty());
        }
        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(p) = patch.dependencies {
            self.dependencies = PlanNode::merged(p);
        }
        if let Some(p) = patch.duration {
            self.duration = p;
        }
        Ok(())
    }
}
// }}}

// PlanDelay supporting types {{{
/// Written by the scheduler for every delay it could schedule, see `PlanDelay::place`
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewPlanDelay {
    pub id: Uuid,
    pub name: Option<String>,
    pub parent: Parent,
    pub duration: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanDelayPatch {
    pub id: Uuid,
    /// `Some(None)` clears the name
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub name: Option<Option<String>>,
    pub parent: Option<Parent>,
    pub dependencies: Option<Vec<PlanNode>>,
    pub duration: Option<u32>,
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn delay(duration: u32) -> PlanDelay {
        PlanDelay::from(NewPlanDelay {
            id: Uuid::new_v4(),
            name: None,
            parent: Parent::Plan,
            duration,
        })
    }

    #[test]
    fn names_are_optional_and_clearable() {
        let mut d = delay(1);
        let id = *d.id();
        assert_eq!(d.name(), None);
        let rename = |name| PlanDelayPatch {
            id,
            name,
            parent: None,
            dependencies: None,
            duration: None,
        };
        let patch = rename(Some(Some("Cure".into())));
        d.apply_patch(patch).unwrap();
        assert_eq!(d.name(), Some("Cure"));
        d.apply_patch(rename(None)).unwrap();
        assert_eq!(d.name(), Some("Cure"));
        d.apply_patch(rename(Some(Some(String::new())))).unwrap();
        assert_eq!(d.name(), None);
    }
}
// }}}
