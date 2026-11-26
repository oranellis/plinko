//! A plan turned into what the chart needs: its items arranged in lanes, and the ones that could
//! not be placed because the scheduler gave them no dates.

// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use plinko::{
    availability::day_value::DayValue,
    planner::items::{
        dependency::{Dependency, PlanNode},
        parent::Parent,
    },
    planner::plan::Plan,
    scheduler::state::PlanVolatileState,
};
use uuid::Uuid;

use crate::{
    gantt::arrange::{Arrangement, Entry, Kind, Mark, Placed, arrange},
    model::{self, node_name},
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// The day the chart counts from: today, or the first day of anything before it
    pub origin: NaiveDate,
    pub arrangement: Arrangement,
    pub unplaced: Vec<Unplaced>,
    /// The day offset after the last day anything is on
    pub last_day: f64,
    /// Where each item is in the arrangement
    index: HashMap<Mark, usize>,
}

// Scene supporting types {{{
/// An item with no place on the chart
#[derive(Clone, Debug, PartialEq)]
pub struct Unplaced {
    pub node: PlanNode,
    pub name: String,
    pub reason: String,
}

struct Context<'a> {
    plan: &'a Plan,
    state: Option<&'a PlanVolatileState>,
    collapsed: &'a HashSet<Uuid>,
    depended: &'a HashSet<PlanNode>,
    targets: &'a HashSet<PlanNode>,
    children: &'a HashMap<Parent, Vec<PlanNode>>,
}
// }}}

// impl Scene {{{
impl Scene {
    /// A scene with nothing in it
    pub fn empty(origin: NaiveDate) -> Self {
        Self {
            origin,
            arrangement: Arrangement::default(),
            unplaced: Vec::new(),
            last_day: 0.0,
            index: HashMap::new(),
        }
    }

    pub fn placed(&self, mark: Mark) -> Option<&Placed> {
        self.arrangement.placed.get(*self.index.get(&mark)?)
    }
}
// }}}

// kind_of {{{
fn kind_of(node: PlanNode) -> Kind {
    match node {
        PlanNode::Task(_) => Kind::Task,
        PlanNode::Milestone(_) => Kind::Milestone,
        PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => Kind::Buffer,
        PlanNode::TaskGroup(_) => Kind::Group,
    }
}
// }}}

// build {{{
pub fn build(
    plan: &Plan,
    state: Option<&PlanVolatileState>,
    collapsed: &HashSet<Uuid>,
    today: NaiveDate,
) -> Scene {
    let nodes = model::nodes(plan);
    let mut depended: HashSet<PlanNode> = HashSet::new();
    for node in &nodes {
        for dep in model::dependencies(plan, *node) {
            if let Dependency::Node(before) = dep {
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
    let entries = context.entries(Parent::Plan);
    let arrangement = arrange(&entries);
    let first = arrangement.placed.iter().map(|p| p.span.0).min();
    let origin = first.map_or(today, |first| first.min(today));
    let last = arrangement
        .placed
        .iter()
        .map(|p| p.span.1)
        .max()
        .unwrap_or(origin);
    let index = arrangement
        .placed
        .iter()
        .enumerate()
        .map(|(i, p)| (p.mark, i))
        .collect();
    Scene {
        origin,
        last_day: (last - origin).num_days() as f64,
        arrangement,
        unplaced,
        index,
    }
}
// }}}

// impl Context {{{
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
                let (name, span) = match node {
                    PlanNode::PlanBuffer(id) => self.buffer(id, span),
                    // a delay with no name is told from a buffer by its shape, so only its days
                    PlanNode::PlanDelay(id) => match self.plan.plan_delays().get(&id) {
                        Some(delay) if delay.name().is_none() => {
                            (format!("{}d", delay.duration()), span)
                        }
                        _ => (node_name(self.plan, node), span),
                    },
                    _ => (node_name(self.plan, node), span),
                };
                Some(Entry {
                    node,
                    name,
                    kind: kind_of(node),
                    span,
                    look: model::look_of(self.plan, self.state, node),
                    violated: self.state.is_some_and(|s| s.constraint_violations.contains_key(&node)),
                    // what ends its group is not a leaf when something waits on the group
                    leaf: !self.depended.contains(&node)
                        && !model::ends_groups(self.plan, self.state, node)
                            .iter()
                            .any(|group| self.depended.contains(group)),
                    rooted: model::parent_of(self.plan, node) == Parent::Plan
                        && model::dependencies(self.plan, node)
                            .iter()
                            .all(|d| *d == Dependency::ParentStart),
                    pinned: matches!(node, PlanNode::PlanBuffer(id)
                        if self.plan.plan_buffers().get(&id).is_some_and(|b| b.set_start_day().is_some())),
                    target: self.targets.contains(&node),
                    members,
                    collapsed: matches!(node, PlanNode::TaskGroup(id) if self.collapsed.contains(&id)),
                })
            })
            .collect()
    }
}

impl Context<'_> {
    /// What a buffer reads as on the chart, its working days now and what it was given when they
    /// differ, and the span it gets: a buffer with no days left still takes a day to click on
    fn buffer(&self, id: Uuid, span: (NaiveDate, NaiveDate)) -> (String, (NaiveDate, NaiveDate)) {
        let availability = self.plan.default_availability();
        let now = span
            .0
            .iter_days()
            .take_while(|d| *d < span.1)
            .filter(|d| availability.value_on(*d).is_working())
            .count();
        let given = self
            .plan
            .plan_buffers()
            .get(&id)
            .map_or(0, |b| *b.duration() as usize);
        let name = if now == given {
            format!("{now}d")
        } else {
            format!("{now}d ({given}d)")
        };
        (name, (span.0, span.1.max(span.0 + chrono::Days::new(1))))
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use plinko::{
        common::{crud::Crud, dates::DateRange, short_id::ShortId},
        planner::items::{
            dependency::Dependency,
            task::{self, NewTask, TaskPatch},
        },
        planner::{plan::NewPlan, plan_update::PlanUpdate},
        scheduler::state::UnscheduledReason,
    };

    use super::*;

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
        });
        let ids: Vec<Uuid> = (1..=3).map(Uuid::from_u128).collect();
        for (i, id) in ids.iter().enumerate() {
            let new = NewTask {
                id: *id,
                name: format!("T{i}"),
                parent: Parent::Plan,
            };
            plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        }
        // T1 waits for T0
        let patch = TaskPatch {
            id: ids[1],
            name: None,
            parent: None,
            duration: None,
            time_constraint: None,
            dependencies: Some(vec![Dependency::Node(PlanNode::Task(ids[0]))]),
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
            .unwrap();
        let mut state = PlanVolatileState::default();
        scheduled(&mut state, ids[0], 3, 5);
        scheduled(&mut state, ids[1], 6, 8);
        state
            .unscheduled
            .insert(PlanNode::Task(ids[2]), UnscheduledReason::NoEligibleWorkers);

        let scene = build(&plan, Some(&state), &HashSet::new(), d(1));
        assert_eq!(scene.origin, d(1));
        let get = |name: &str| {
            scene
                .arrangement
                .placed
                .iter()
                .find(|p| p.name == name)
                .unwrap()
        };
        // T0 and T1 in the first lane with a day between them
        assert_eq!(scene.arrangement.placed.len(), 2);
        assert_eq!((get("T0").lane, get("T1").lane), (0, 0));
        assert!(!get("T0").leaf && get("T1").leaf);
        assert_eq!(scene.last_day, 7.0);
        assert_eq!(scene.unplaced.len(), 1);
        assert_eq!(scene.unplaced[0].name, "T2");
        assert!(scene.unplaced[0].reason.contains("eligible"));
    }
}
// }}}
