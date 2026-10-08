//! A plan turned into what the chart needs: its items arranged in lanes, and the ones that could
//! not be placed because the scheduler gave them no dates.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use plinko::{
    planner::plan::Plan,
    scheduler::state::PlanVolatileState,
    types::{
        dependency::{DependencyNode, PlanNode},
        parent::Parent,
    },
};
use uuid::Uuid;

use crate::{
    gantt::arrange::{Arrangement, Entry, Kind, arrange},
    model::{self, node_name},
};

/// An item with no place on the chart
#[derive(Clone, Debug, PartialEq)]
pub struct Unplaced {
    pub node: PlanNode,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub plan_start: NaiveDate,
    pub arrangement: Arrangement,
    pub unplaced: Vec<Unplaced>,
    /// The day offset after the last day anything is on
    pub last_day: f64,
}

fn kind_of(node: PlanNode) -> Kind {
    match node {
        PlanNode::Task(_) => Kind::Task,
        PlanNode::Milestone(_) => Kind::Milestone,
        PlanNode::PlanBuffer(_) => Kind::Buffer,
        PlanNode::TaskGroup(_) => Kind::Group,
    }
}

pub fn build(plan: &Plan, state: Option<&PlanVolatileState>, collapsed: &HashSet<Uuid>) -> Scene {
    let plan_start = *plan.scheduler_data().start_date();
    let nodes = model::nodes(plan);
    let mut depended: HashSet<PlanNode> = HashSet::new();
    for node in &nodes {
        for dep in model::dependencies(plan, *node) {
            if let DependencyNode::Node(before) = dep.node {
                depended.insert(before);
            }
        }
    }
    let targets: HashSet<PlanNode> = plan.targets().values().map(|t| t.node()).collect();
    let mut children: HashMap<Parent, Vec<PlanNode>> = HashMap::new();
    let mut unplaced = Vec::new();
    for node in nodes {
        if model::span_of(state, node).is_none() {
            let reason = state
                .and_then(|s| s.unscheduled.get(&node))
                .map_or("Not scheduled yet", |r| model::reason(*r));
            unplaced.push(Unplaced {
                node,
                name: node_name(plan, node),
                reason: reason.to_string(),
            });
            continue;
        }
        children
            .entry(model::parent_of(plan, node))
            .or_default()
            .push(node);
    }
    unplaced.sort_by(|a, b| a.name.cmp(&b.name).then(a.node.id().cmp(&b.node.id())));

    let context = Context {
        plan,
        state,
        collapsed,
        depended: &depended,
        targets: &targets,
        children: &children,
    };
    let entries = context.entries(Parent::PlanStart);
    let arrangement = arrange(plan_start, &entries);
    let last = arrangement
        .placed
        .iter()
        .map(|p| p.span.1)
        .max()
        .unwrap_or(plan_start);
    Scene {
        plan_start,
        last_day: (last - plan_start).num_days() as f64,
        arrangement,
        unplaced,
    }
}

struct Context<'a> {
    plan: &'a Plan,
    state: Option<&'a PlanVolatileState>,
    collapsed: &'a HashSet<Uuid>,
    depended: &'a HashSet<PlanNode>,
    targets: &'a HashSet<PlanNode>,
    children: &'a HashMap<Parent, Vec<PlanNode>>,
}

impl Context<'_> {
    fn entries(&self, parent: Parent) -> Vec<Entry> {
        let nodes = self.children.get(&parent).into_iter().flatten().copied();
        nodes
            .filter_map(|node| {
                let span = model::span_of(self.state, node)?;
                let members = match node {
                    PlanNode::TaskGroup(id) => self.entries(Parent::Node(id)),
                    _ => Vec::new(),
                };
                Some(Entry {
                    node,
                    name: node_name(self.plan, node),
                    kind: kind_of(node),
                    span,
                    look: model::look_of(self.plan, self.state, node),
                    violated: self.state.is_some_and(|s| s.constraint_violations.contains_key(&node)),
                    // what ends its group is not a leaf when something waits on the group
                    leaf: !self.depended.contains(&node)
                        && !model::ends_groups(self.plan, self.state, node)
                            .iter()
                            .any(|group| self.depended.contains(group)),
                    target: self.targets.contains(&node),
                    members,
                    collapsed: matches!(node, PlanNode::TaskGroup(id) if self.collapsed.contains(&id)),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use plinko::{
        planner::{plan::NewPlan, plan_update::PlanUpdate},
        scheduler::state::UnscheduledReason,
        types::{
            common::DateRange,
            crud::Crud,
            dependency::Dependency,
            short_id::ShortId,
            task::{self, NewTask, TaskPatch},
        },
    };

    use super::*;
    use crate::gantt::arrange::Mark;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn scheduled(state: &mut PlanVolatileState, id: Uuid, from: u32, to: u32) {
        state.tasks.insert(
            id,
            task::VolatileState {
                date_range: DateRange::new(d(from), d(to)).unwrap(),
                assignments: Default::default(),
                workers: Default::default(),
            },
        );
    }

    #[test]
    fn items_get_lanes_leaves_are_marked_and_the_unscheduled_are_listed() {
        let mut plan = Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
            start_date: d(1),
        });
        let ids: Vec<Uuid> = (1..=3).map(Uuid::from_u128).collect();
        for (i, id) in ids.iter().enumerate() {
            let new = NewTask {
                id: *id,
                name: format!("T{i}"),
                parent: Parent::PlanStart,
            };
            plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        }
        // T1 waits for T0
        let patch = TaskPatch {
            id: ids[1],
            name: None,
            parent: None,
            mode: None,
            duration: None,
            time_constraint: None,
            dependencies: Some(vec![Dependency {
                node: DependencyNode::Node(PlanNode::Task(ids[0])),
                lag: 0,
            }]),
            allocations: None,
            affinities: None,
            actual_workers: None,
            description: None,
            persistent_state: None,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
            .unwrap();
        let mut state = PlanVolatileState::default();
        scheduled(&mut state, ids[0], 3, 5);
        scheduled(&mut state, ids[1], 6, 8);
        state
            .unscheduled
            .insert(PlanNode::Task(ids[2]), UnscheduledReason::NoPathToPlanStart);

        let scene = build(&plan, Some(&state), &HashSet::new());
        assert_eq!(scene.plan_start, d(1));
        let get = |name: &str| {
            scene
                .arrangement
                .placed
                .iter()
                .find(|p| p.name == name)
                .unwrap()
        };
        // the plan start, then T0 and T1 in the first lane with a day between them
        assert_eq!(scene.arrangement.placed.len(), 3);
        assert_eq!((get("T0").lane, get("T1").lane), (0, 0));
        assert!(!get("T0").leaf && get("T1").leaf);
        assert_eq!(get("Plan Start").mark, Mark::PlanStart);
        assert_eq!(scene.last_day, 7.0);
        assert_eq!(scene.unplaced.len(), 1);
        assert_eq!(scene.unplaced[0].name, "T2");
        assert!(scene.unplaced[0].reason.contains("connects"));
    }
}
