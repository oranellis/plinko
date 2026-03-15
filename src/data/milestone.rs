use serde::{Deserialize, Serialize};

use crate::data::{DateConstraint, Dependency, MilestoneId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: MilestoneId,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    pub constraint: Option<DateConstraint>,
}

impl Milestone {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: MilestoneId::new(),
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            constraint: None,
        }
    }
}
