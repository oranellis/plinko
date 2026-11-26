//! The dependency lines shown for the item under the pointer: what it waits for, and what waits
//! for it. Which ends they join is worked out here and where they are is drawn from it.

// modules {{{
use std::collections::HashMap;

use plinko::{
    planner::items::{
        dependency::{Dependency, PlanNode},
        parent::Parent,
    },
    planner::plan::Plan,
    scheduler::state::PlanVolatileState,
};

use crate::{gantt::arrange::Mark, model};
// }}}

/// A place a line can end
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// The first day of an item, a group's left edge
    Start(PlanNode),
    /// The last day of an item, a group's right edge
    End(PlanNode),
}

// Anchor supporting types {{{
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Comes before the item under the pointer
    Before,
    /// Comes after it
    After,
    /// Sits on the longest chain between the selected item and the one under the pointer
    Chain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: Anchor,
    pub to: Anchor,
    pub role: Role,
}
// }}}

// links {{{
/// Only the direct links of the item, one step either way. An item inside a group that depends on
/// the start of its group is linked to the start of the group. At the top level that is no line,
/// the item just begins when the plan does.
pub fn links(plan: &Plan, state: Option<&PlanVolatileState>, hovered: Mark) -> Vec<Link> {
    let mut out = Vec::new();
    let nodes = model::nodes(plan);
    let node = hovered;
    let mut deps = model::dependencies(plan, node);
    if deps.is_empty() {
        // depending on nothing is depending on the start of the parent
        deps.push(Dependency::default());
    }
    for dep in deps {
        let from = match dep {
            Dependency::Node(before) => Anchor::End(before),
            Dependency::ParentStart => match model::parent_of(plan, node) {
                Parent::Plan => continue,
                Parent::Node(group) => Anchor::Start(PlanNode::TaskGroup(group)),
            },
        };
        out.push(Link {
            from,
            to: Anchor::Start(node),
            role: Role::Before,
        });
    }
    for other in nodes {
        if model::dependencies(plan, other).contains(&Dependency::Node(node)) {
            out.push(Link {
                from: Anchor::End(node),
                to: Anchor::Start(other),
                role: Role::After,
            });
        }
    }
    // what ends a group ends it, and the group ends with it
    if let Some(group) = model::ends_groups(plan, state, node).first().copied() {
        out.push(Link {
            from: Anchor::End(node),
            to: Anchor::End(group),
            role: Role::After,
        });
    }
    out
}
// }}}

// chain {{{
/// The longest chain of dependencies between two items, in either direction, as links
pub fn chain(plan: &Plan, a: PlanNode, b: PlanNode) -> Vec<Link> {
    let nodes = model::nodes(plan);
    let longest = |from: PlanNode, to: PlanNode| {
        let mut known = HashMap::new();
        path(plan, &nodes, from, to, &mut known)
    };
    let nodes = [longest(a, b), longest(b, a)]
        .into_iter()
        .flatten()
        .max_by_key(Vec::len)
        .unwrap_or_default();
    nodes
        .windows(2)
        .map(|w| Link {
            from: Anchor::End(w[0]),
            to: Anchor::Start(w[1]),
            role: Role::Chain,
        })
        .collect()
}
// }}}

// path {{{
/// The longest run of items from `from` to `to`, each waiting on the one before
fn path(
    plan: &Plan,
    nodes: &[PlanNode],
    from: PlanNode,
    to: PlanNode,
    known: &mut HashMap<PlanNode, Option<Vec<PlanNode>>>,
) -> Option<Vec<PlanNode>> {
    if from == to {
        return Some(vec![from]);
    }
    if let Some(done) = known.get(&from) {
        return done.clone();
    }
    let mut best: Option<Vec<PlanNode>> = None;
    for next in nodes
        .iter()
        .filter(|n| model::dependencies(plan, **n).contains(&Dependency::Node(from)))
    {
        if let Some(rest) = path(plan, nodes, *next, to, known)
            && best.as_ref().is_none_or(|b| rest.len() + 1 > b.len())
        {
            best = Some([vec![from], rest].concat());
        }
    }
    known.insert(from, best.clone());
    best
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use plinko::{
        common::{crud::Crud, short_id::ShortId},
        planner::items::{
            dependency::Dependency,
            task::{NewTask, TaskPatch},
            task_group::NewTaskGroup,
        },
        planner::{plan::NewPlan, plan_update::PlanUpdate},
    };
    use uuid::Uuid;

    use super::*;

    fn task(plan: &mut Plan, id: u128, parent: Parent, on: &[Dependency]) -> PlanNode {
        let id = Uuid::from_u128(id);
        let new = NewTask {
            id,
            name: "T".to_string(),
            parent,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        let patch = TaskPatch {
            id,
            name: None,
            parent: None,
            duration: None,
            time_constraint: None,
            dependencies: Some(on.to_vec()),
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
            .unwrap();
        PlanNode::Task(id)
    }

    fn new_plan() -> Plan {
        Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        })
    }

    #[test]
    fn an_item_is_linked_to_what_it_waits_for_and_what_waits_for_it() {
        let mut plan = new_plan();
        let first = task(&mut plan, 1, Parent::Plan, &[Dependency::ParentStart]);
        let second = task(&mut plan, 2, Parent::Plan, &[Dependency::Node(first)]);
        let third = task(&mut plan, 3, Parent::Plan, &[Dependency::Node(second)]);

        let around = links(&plan, None, second);
        assert_eq!(around.len(), 2);
        assert!(around.contains(&Link {
            from: Anchor::End(first),
            to: Anchor::Start(second),
            role: Role::Before
        }));
        assert!(around.contains(&Link {
            from: Anchor::End(second),
            to: Anchor::Start(third),
            role: Role::After
        }));

        // the first begins when the plan does, which is no line
        assert!(
            links(&plan, None, first)
                .iter()
                .all(|l| l.role != Role::Before)
        );
    }

    #[test]
    fn the_chain_between_two_items_is_the_longest_either_way_round() {
        let mut plan = new_plan();
        let a = task(&mut plan, 1, Parent::Plan, &[]);
        let b = task(&mut plan, 2, Parent::Plan, &[Dependency::Node(a)]);
        let c = task(&mut plan, 3, Parent::Plan, &[Dependency::Node(b)]);
        let d = task(
            &mut plan,
            4,
            Parent::Plan,
            &[Dependency::Node(a), Dependency::Node(c)],
        );
        let step = |from, to| Link {
            from: Anchor::End(from),
            to: Anchor::Start(to),
            role: Role::Chain,
        };
        // not the short way a to d
        let long = [step(a, b), step(b, c), step(c, d)];
        assert_eq!(chain(&plan, a, d), long);
        assert_eq!(chain(&plan, d, a), long);
        assert!(chain(&plan, b, b).is_empty());
        let lone = task(&mut plan, 5, Parent::Plan, &[]);
        assert!(chain(&plan, a, lone).is_empty());
    }

    #[test]
    fn items_in_a_group_start_from_the_start_of_the_group() {
        let mut plan = new_plan();
        let id = Uuid::from_u128(10);
        let new = NewTaskGroup {
            id,
            name: "G".to_string(),
            parent: Parent::Plan,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
            .unwrap();
        let group = PlanNode::TaskGroup(id);
        let member = task(&mut plan, 1, Parent::Node(id), &[Dependency::ParentStart]);
        let after = task(&mut plan, 2, Parent::Plan, &[Dependency::Node(group)]);

        assert!(links(&plan, None, member).contains(&Link {
            from: Anchor::Start(group),
            to: Anchor::Start(member),
            role: Role::Before
        }));
        let around = links(&plan, None, group);
        // what is inside a group is not linked to it
        assert!(!around.iter().any(|l| l.to == Anchor::Start(member)));
        assert!(around.contains(&Link {
            from: Anchor::End(group),
            to: Anchor::Start(after),
            role: Role::After
        }));
    }
}
// }}}
