use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What an item is nested under: the plan itself (top level), or a task group
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Parent {
    #[default]
    #[serde(alias = "PlanStart")]
    Plan,
    Node(Uuid),
}
