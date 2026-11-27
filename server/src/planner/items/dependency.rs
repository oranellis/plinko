// modules{{{
use serde::{Deserialize, Serialize};
use std::hash::Hash;

use uuid::Uuid;
// }}}

/// A schedulable node. A task group is a single node that ends when all of its members end.
///
/// An item's dependencies are the nodes whose end it waits for. Besides them every item waits
/// for the start of the group it is in, or for today at the top level, which is never listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanBuffer(Uuid),
    PlanDelay(Uuid),
    TaskGroup(Uuid),
}

// PlanNode supporting types {{{
/// The kind of a node, without which one it is
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanNodeType {
    Task,
    Milestone,
    PlanBuffer,
    PlanDelay,
    TaskGroup,
}
// }}}

// impl PlanNode {{{
impl PlanNode {
    pub fn id(&self) -> Uuid {
        match *self {
            PlanNode::Task(id)
            | PlanNode::Milestone(id)
            | PlanNode::PlanBuffer(id)
            | PlanNode::PlanDelay(id)
            | PlanNode::TaskGroup(id) => id,
        }
    }

    pub fn node_type(&self) -> PlanNodeType {
        match self {
            PlanNode::Task(_) => PlanNodeType::Task,
            PlanNode::Milestone(_) => PlanNodeType::Milestone,
            PlanNode::PlanBuffer(_) => PlanNodeType::PlanBuffer,
            PlanNode::PlanDelay(_) => PlanNodeType::PlanDelay,
            PlanNode::TaskGroup(_) => PlanNodeType::TaskGroup,
        }
    }

    /// Each node once, in the order first given
    pub fn merged(nodes: Vec<PlanNode>) -> Vec<PlanNode> {
        let mut out: Vec<PlanNode> = Vec::new();
        for node in nodes {
            if !out.contains(&node) {
                out.push(node);
            }
        }
        out
    }
}
// }}}

// impl PlanNodeType {{{
impl PlanNodeType {
    pub fn node(self, id: Uuid) -> PlanNode {
        match self {
            PlanNodeType::Task => PlanNode::Task(id),
            PlanNodeType::Milestone => PlanNode::Milestone(id),
            PlanNodeType::PlanBuffer => PlanNode::PlanBuffer(id),
            PlanNodeType::PlanDelay => PlanNode::PlanDelay(id),
            PlanNodeType::TaskGroup => PlanNode::TaskGroup(id),
        }
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_node_is_kept_once() {
        let on = |id| PlanNode::Task(id);
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(
            PlanNode::merged(vec![on(a), on(b), on(a)]),
            vec![on(a), on(b)]
        );
    }
}
// }}}
