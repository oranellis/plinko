//! Moving an item into, out of or between task groups without breaking the chains around it.

// modules {{{
use crate::{
    planner::items::{
        dependency::{Dependency, DependencyNode, PlanNode},
        parent::Parent,
    },
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

    /// The updates that put `node` in the group `to`, or at the top for `PlanStart`. Leaving a
    /// group the item takes the group's dependencies as its own start. Entering one, what the item
    /// waited on moves onto the group, what waited on the item outside the group waits on the
    /// group's end, and the item waits on the group's start.
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
            let (_, outer) = placement(self, PlanNode::TaskGroup(id));
            deps = Dependency::merged(
                deps.into_iter()
                    .flat_map(|d| match d.node {
                        DependencyNode::ParentStart => outer
                            .iter()
                            .map(|o| Dependency {
                                node: o.node,
                                lag: o.lag + d.lag,
                            })
                            .collect(),
                        DependencyNode::Node(_) => vec![d],
                    })
                    .collect(),
            );
            if deps.is_empty() {
                deps.push(Dependency::default());
            }
        }
        if let Parent::Node(id) = to {
            let group = PlanNode::TaskGroup(id);
            let (lifted, inside): (Vec<_>, Vec<_>) = deps.into_iter().partition(|d| match d.node {
                DependencyNode::ParentStart => false,
                DependencyNode::Node(n) => !self.is_within(n, group),
            });
            let (_, own) = placement(self, group);
            let mut group_deps = Dependency::merged([own, lifted].concat());
            // waiting on the parent's start is implied once there is something else to wait on
            if group_deps.len() > 1 {
                group_deps.retain(|d| d.node != DependencyNode::ParentStart || d.lag != 0);
            }
            updates.push(PlanUpdate::set_dependencies(group, group_deps));
            deps = [vec![Dependency::default()], inside].concat();
            let mine = DependencyNode::Node(node);
            let ends = Dependency {
                node: DependencyNode::Node(group),
                lag: 0,
            };
            for other in all_nodes(self)
                .into_iter()
                .filter(|n| *n != node && !self.is_within(*n, group))
            {
                let (_, theirs) = placement(self, other);
                if theirs.iter().any(|d| d.node == mine) {
                    let swapped = theirs
                        .into_iter()
                        .map(|d| {
                            if d.node == mine {
                                Dependency { lag: d.lag, ..ends }
                            } else {
                                d
                            }
                        })
                        .collect();
                    updates.push(PlanUpdate::rewire(
                        other,
                        None,
                        Some(Dependency::merged(swapped)),
                    ));
                }
            }
        }
        updates.insert(0, PlanUpdate::rewire(node, Some(to), Some(deps)));
        Ok(updates)
    }
}
// }}}
