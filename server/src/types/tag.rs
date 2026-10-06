use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};

#[derive(Debug, PartialEq)]
pub struct Tag {
    id: Uuid,
    name: String,
}

// impl Tag{{{
impl Tag {
    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl From<NewTag> for Tag {
    fn from(new: NewTag) -> Self {
        Self {
            id: new.id,
            name: new.name,
        }
    }
}

impl Patchable<TagPatch> for Tag {
    fn apply_patch(&mut self, patch: TagPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;
        if let Some(n) = patch.name {
            self.name = n;
        }
        Ok(())
    }
}
// }}}

pub struct NewTag {
    pub id: Uuid,
    pub name: String,
}

// Bit weaksauce because there is only one thing you can change, maybe later I'll add groups or
// something which will make this worth it
pub struct TagPatch {
    pub id: Uuid,
    pub name: Option<String>,
}
