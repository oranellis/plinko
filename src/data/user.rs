use crate::data::ids::{TagId, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub tags: HashSet<TagId>,
}

impl User {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: UserId::new(),
            name: name.into(),
            tags: HashSet::new(),
        }
    }

    pub fn with_tag(mut self, tag_id: TagId) -> Self {
        self.tags.insert(tag_id);
        self
    }

    pub fn add_tag(&mut self, tag_id: TagId) {
        self.tags.insert(tag_id);
    }

    pub fn remove_tag(&mut self, tag_id: &TagId) {
        self.tags.remove(tag_id);
    }

    pub fn has_tag(&self, tag_id: &TagId) -> bool {
        self.tags.contains(tag_id)
    }
}
