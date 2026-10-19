// modules{{{
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    common::error::PlinkoResult,
    helpers::functions::check_id_match,
    traits::{entity::Entity, patch::Patchable},
};
// }}}

/// Optional extra information attached to a task or milestone
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DataBlocks {
    blocks: HashMap<Uuid, DataBlock>,
}

// impl DataBlocks {{{
impl DataBlocks {
    pub fn blocks(&self) -> &HashMap<Uuid, DataBlock> {
        &self.blocks
    }
}
// }}}

// DataBlocks supporting types {{{
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataBlock {
    id: Uuid,
    title: String,
    content: BlockContent,
}

// impl DataBlock {{{
impl DataBlock {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn content(&self) -> &BlockContent {
        &self.content
    }
}

impl From<NewDataBlock> for DataBlock {
    fn from(new: NewDataBlock) -> Self {
        Self {
            id: new.id,
            title: new.title,
            content: new.content,
        }
    }
}

impl Patchable<DataBlockPatch> for DataBlock {
    fn apply_patch(&mut self, patch: DataBlockPatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(t) = patch.title {
            self.title = t;
        }
        if let Some(c) = patch.content {
            self.content = c;
        }
        Ok(())
    }
}

impl Entity<DataBlocks> for DataBlock {
    type New = NewDataBlock;
    type Patch = DataBlockPatch;

    fn entities(blocks: &DataBlocks) -> &HashMap<Uuid, Self> {
        &blocks.blocks
    }
    fn entities_mut(blocks: &mut DataBlocks) -> &mut HashMap<Uuid, Self> {
        &mut blocks.blocks
    }
}
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockContent {
    TodoList(Vec<TodoItem>),
    Links(Vec<Link>),
    Budget(Budget),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub url: String,
}

/// Money in minor units (like cents) of a single currency
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub planned: i64,
    pub actual: i64,
    pub currency: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewDataBlock {
    pub id: Uuid,
    pub title: String,
    pub content: BlockContent,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DataBlockPatch {
    pub id: Uuid,
    pub title: Option<String>,
    pub content: Option<BlockContent>,
}
// }}}
