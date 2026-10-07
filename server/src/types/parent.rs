use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What an item is nested under: the plan itself, or a task group
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Parent {
    #[default]
    PlanStart,
    Node(Uuid),
}
