// modules {{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{dependency::PlanNode, error::PlinkoResult},
};
// }}}

/// A node the scheduler brings as far forward as it can. It has no due date, the scheduler
/// forecasts when it lands. `priority` is added to the plan's and breaks contention for people.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    id: Uuid,
    node: PlanNode,
    priority: i32,
}

// impl Target {{{
impl Target {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn node(&self) -> PlanNode {
        self.node
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }
}

impl From<NewTarget> for Target {
    fn from(new: NewTarget) -> Self {
        Self {
            id: new.id,
            node: new.node,
            priority: new.priority,
        }
    }
}

impl Patchable<TargetPatch> for Target {
    fn apply_patch(&mut self, patch: TargetPatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(n) = patch.node {
            self.node = n;
        }
        if let Some(p) = patch.priority {
            self.priority = p;
        }
        Ok(())
    }
}
// }}}

// Target supporting types {{{
#[derive(Clone, Serialize, Deserialize)]
pub struct NewTarget {
    pub id: Uuid,
    pub node: PlanNode,
    pub priority: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TargetPatch {
    pub id: Uuid,
    pub node: Option<PlanNode>,
    pub priority: Option<i32>,
}
// }}}
