use crate::data::ids::TagId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
}

impl Tag {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TagId::new(),
            name: name.into(),
        }
    }
}
