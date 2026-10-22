// modules{{{
use serde::{Deserialize, Serialize};
use std::hash::Hash;

use uuid::Uuid;
// }}}

/// What an item waits for, the end of another node or the start of its group
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dependency {
    /// The start of the enclosing task group, or today for top level items
    #[default]
    ParentStart,
    Node(PlanNode),
}

// impl Dependency {{{
impl Dependency {
    /// Each node once, in the order first given
    pub fn merged(deps: Vec<Dependency>) -> Vec<Dependency> {
        let mut out: Vec<Dependency> = Vec::new();
        for dep in deps {
            if !out.contains(&dep) {
                out.push(dep);
            }
        }
        out
    }
}
// }}}

// Dependency supporting types {{{
/// A schedulable node. A task group is a single node that ends when all of its members end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanBuffer(Uuid),
    PlanDelay(Uuid),
    TaskGroup(Uuid),
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
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_node_is_kept_once() {
        let on = |id| Dependency::Node(PlanNode::Task(id));
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(
            Dependency::merged(vec![on(a), on(b), on(a)]),
            vec![on(a), on(b)]
        );
    }
}
// }}}
