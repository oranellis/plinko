// modules{{{
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::rule_set::{RuleSet, RuleSetUpdate},
    common::error::PlinkoResult,
    helpers::functions::check_id_match,
    traits::patch::Patchable,
};
// }}}

/// Days that linked tasks may be worked on. Every day is allowed until rules change it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affinity {
    id: Uuid,
    name: String,
    rules: RuleSet<bool>,
}

// impl Affinity {{{
impl Affinity {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn rules(&self) -> &RuleSet<bool> {
        &self.rules
    }

    pub fn allows(&self, day: NaiveDate) -> bool {
        self.rules.value_on(day)
    }

    pub fn apply_rule_update(&mut self, update: RuleSetUpdate<bool>) -> PlinkoResult<()> {
        self.rules.apply_update(update)
    }
}

impl From<NewAffinity> for Affinity {
    fn from(new: NewAffinity) -> Self {
        Self {
            id: new.id,
            name: new.name,
            rules: RuleSet::default(),
        }
    }
}

impl Patchable<AffinityPatch> for Affinity {
    fn apply_patch(&mut self, patch: AffinityPatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
        Ok(())
    }
}
// }}}

// Affinity supporting types {{{
#[derive(Clone, Serialize, Deserialize)]
pub struct NewAffinity {
    pub id: Uuid,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AffinityPatch {
    pub id: Uuid,
    pub name: Option<String>,
}
// }}}
