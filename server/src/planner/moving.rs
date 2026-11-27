//! Moving an item into, out of or between task groups without breaking the chains around it.

// modules {{{
use crate::{
    planner::items::{dependency::PlanNode, parent::Parent},
    planner::{
        plan::Plan,
        plan_update::PlanUpdate,
        removal::{all_nodes, placement},
    },
};
// }}}

// impl Plan {{{
impl Plan {
    /// Whether `node` is `group` or sits somewhere inside it
    fn is_within(&self, node: PlanNode, group: PlanNode) -> bool {
        let mut at = node;
        loop {
            if at == group {
                return true;
            }
            match placement(self, at).0 {
                Parent::Node(id) => at = PlanNode::TaskGroup(id),
                Parent::Plan => return false,
            }
        }
    }

    /// The updates that put `node` in the group `to`, or at the top for `Parent::Plan`. Leaving a
    /// group the item takes the group's dependencies as its own, as it waited for the group's
    /// start. Entering one, what the item waited on outside it moves onto the group, and what
    /// waited on the item outside the group waits on the group's end.
    pub fn moving(&self, node: PlanNode, to: Parent) -> Result<Vec<PlanUpdate>, String> {
        let (from, mut deps) = placement(self, node);
        if from == to {
            return Ok(Vec::new());
        }
        if let Parent::Node(id) = to
            && self.is_within(PlanNode::TaskGroup(id), node)
        {
            return Err("a group cannot go inside itself".into());
        }
        let mut updates = Vec::new();
        if let Parent::Node(id) = from {
            // it waited for the group's start, so it waits for what the group waits for
            let (_, outer) = placement(self, PlanNode::TaskGroup(id));
            deps = PlanNode::merged([deps, outer].concat());
        }
        if let Parent::Node(id) = to {
            let group = PlanNode::TaskGroup(id);
            let (lifted, inside): (Vec<_>, Vec<_>) =
                deps.into_iter().partition(|n| !self.is_within(*n, group));
            let (_, own) = placement(self, group);
            let group_deps = PlanNode::merged([own, lifted].concat());
            updates.push(PlanUpdate::set_dependencies(group, group_deps));
            deps = inside;
            let mine = node;
            let ends = group;
            for other in all_nodes(self)
                .into_iter()
                .filter(|n| *n != node && !self.is_within(*n, group))
            {
                let (_, theirs) = placement(self, other);
                if theirs.contains(&mine) {
                    let swapped = theirs
                        .into_iter()
                        .map(|d| if d == mine { ends } else { d })
                        .collect();
                    updates.push(PlanUpdate::rewire(
                        other,
                        None,
                        Some(PlanNode::merged(swapped)),
                    ));
                }
            }
        }
        updates.insert(0, PlanUpdate::rewire(node, Some(to), Some(deps)));
        Ok(updates)
    }
}
// }}}
