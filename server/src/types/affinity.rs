// modules{{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::rule_set::{RuleSet, RuleSetUpdate},
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::error::PlinkoResult,
};
// }}}

/// A named set of days that tasks linked to it may be worked on, described with the same rules
/// as availability but with a yes or no for each day. Until rules change it, every day is allowed.
#[derive(Clone, Debug, PartialEq)]
pub struct Affinity {
    id: Uuid,
    name: String,
    rules: RuleSet<bool>,
}

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

pub struct NewAffinity {
    pub id: Uuid,
    pub name: String,
}

pub struct AffinityPatch {
    pub id: Uuid,
    pub name: Option<String>,
}
