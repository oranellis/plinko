use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `Plan` is the top level, `Node` a task group
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Parent {
    #[default]
    #[serde(alias = "PlanStart")]
    Plan,
    Node(Uuid),
}
