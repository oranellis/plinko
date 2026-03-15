use crate::data::ids::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub id: NodeId,
    pub lag_days: f32,
}

impl Dependency {
    pub fn new(id: NodeId) -> Self {
        Self { id, lag_days: 0.0 }
    }

    pub fn with_lag(id: NodeId, days: f32) -> Self {
        Self { id, lag_days: days }
    }

    pub fn with_lead(id: NodeId, days: f32) -> Self {
        Self {
            id,
            lag_days: -days.abs(),
        }
    }
}
