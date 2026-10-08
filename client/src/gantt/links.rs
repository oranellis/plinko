//! The dependency lines shown for the item under the pointer: what it waits for, and what waits
//! for it. Which ends they join is worked out here and where they are is drawn from it.

use plinko::{
    planner::plan::Plan,
    types::{
        dependency::{DependencyNode, PlanNode},
        parent::Parent,
    },
};

use crate::{gantt::arrange::Mark, model};

/// A place a line can end
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    PlanStart,
    /// The first day of an item, a group's left edge
    Start(PlanNode),
    /// The last day of an item, a group's right edge
    End(PlanNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Comes before the item under the pointer
    Before,
    /// Comes after it
    After,
    /// Is inside the group under the pointer and starts with it
    Inside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: Anchor,
    pub to: Anchor,
    pub role: Role,
}

/// Only the direct links of the item, one step either way. An item inside a group that depends on
/// the start of its group is linked to the start of the group, and the plan start for the top level.
pub fn links(plan: &Plan, hovered: Mark) -> Vec<Link> {
    let mut out = Vec::new();
    let nodes = model::nodes(plan);
    let start_of = |parent: Parent| match parent {
        Parent::PlanStart => Anchor::PlanStart,
        Parent::Node(group) => Anchor::Start(PlanNode::TaskGroup(group)),
    };
    let Mark::Node(node) = hovered else {
        for other in nodes {
            let top = model::parent_of(plan, other) == Parent::PlanStart;
            if top
                && model::dependencies(plan, other)
                    .iter()
                    .any(|d| d.node == DependencyNode::ParentStart)
            {
                out.push(Link {
                    from: Anchor::PlanStart,
                    to: Anchor::Start(other),
                    role: Role::After,
                });
            }
        }
        return out;
    };
    for dep in model::dependencies(plan, node) {
        let from = match dep.node {
            DependencyNode::Node(before) => Anchor::End(before),
            DependencyNode::ParentStart => start_of(model::parent_of(plan, node)),
        };
        out.push(Link {
            from,
            to: Anchor::Start(node),
            role: Role::Before,
        });
    }
    for other in nodes {
        for dep in model::dependencies(plan, other) {
            let after_end = dep.node == DependencyNode::Node(node);
            let in_group = matches!(node, PlanNode::TaskGroup(group)
                if dep.node == DependencyNode::ParentStart
                    && model::parent_of(plan, other) == Parent::Node(group));
            if after_end || in_group {
                let (from, role) = if after_end {
                    (Anchor::End(node), Role::After)
                } else {
                    (Anchor::Start(node), Role::Inside)
                };
                out.push(Link {
                    from,
                    to: Anchor::Start(other),
                    role,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use plinko::{
        planner::{plan::NewPlan, plan_update::PlanUpdate},
        types::{
            crud::Crud,
            dependency::Dependency,
            short_id::ShortId,
            task::{NewTask, TaskPatch},
            task_group::NewTaskGroup,
        },
    };
    use uuid::Uuid;

    use super::*;

    fn task(plan: &mut Plan, id: u128, parent: Parent, on: &[DependencyNode]) -> PlanNode {
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
            mode: None,
            duration: None,
            time_constraint: None,
            dependencies: Some(on.iter().map(|n| Dependency { node: *n, lag: 0 }).collect()),
            allocations: None,
            affinities: None,
            actual_workers: None,
            description: None,
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
            start_date: chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        })
    }

    #[test]
    fn an_item_is_linked_to_what_it_waits_for_and_what_waits_for_it() {
        let mut plan = new_plan();
        let first = task(
            &mut plan,
            1,
            Parent::PlanStart,
            &[DependencyNode::ParentStart],
        );
        let second = task(
            &mut plan,
            2,
            Parent::PlanStart,
            &[DependencyNode::Node(first)],
        );
        let third = task(
            &mut plan,
            3,
            Parent::PlanStart,
            &[DependencyNode::Node(second)],
        );

        let around = links(&plan, Mark::Node(second));
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

        // the first waits for the plan start, and the plan start is linked to what starts from it
        let start = links(&plan, Mark::Node(first));
        assert!(start.contains(&Link {
            from: Anchor::PlanStart,
            to: Anchor::Start(first),
            role: Role::Before
        }));
        let from_start = links(&plan, Mark::PlanStart);
        assert_eq!(
            from_start,
            [Link {
                from: Anchor::PlanStart,
                to: Anchor::Start(first),
                role: Role::After
            }]
        );
    }

    #[test]
    fn items_in_a_group_start_from_the_start_of_the_group() {
        let mut plan = new_plan();
        let id = Uuid::from_u128(10);
        let new = NewTaskGroup {
            id,
            name: "G".to_string(),
            parent: Parent::PlanStart,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
            .unwrap();
        let group = PlanNode::TaskGroup(id);
        let member = task(
            &mut plan,
            1,
            Parent::Node(id),
            &[DependencyNode::ParentStart],
        );
        let after = task(
            &mut plan,
            2,
            Parent::PlanStart,
            &[DependencyNode::Node(group)],
        );

        assert!(links(&plan, Mark::Node(member)).contains(&Link {
            from: Anchor::Start(group),
            to: Anchor::Start(member),
            role: Role::Before
        }));
        let around = links(&plan, Mark::Node(group));
        assert!(around.contains(&Link {
            from: Anchor::Start(group),
            to: Anchor::Start(member),
            role: Role::Inside
        }));
        assert!(around.contains(&Link {
            from: Anchor::End(group),
            to: Anchor::Start(after),
            role: Role::After
        }));
    }
}
