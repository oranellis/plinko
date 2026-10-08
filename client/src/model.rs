//! A plan as rows to show: the tree of groups and items in order, with where each is scheduled.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    planner::plan::Plan,
    scheduler::state::{PlanVolatileState, UnscheduledReason},
    types::{
        dependency::{Dependency, PlanNode},
        milestone::DerivedStatus,
        parent::Parent,
        task::Status,
    },
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    NotStarted,
    InProgress,
    OnHold,
    Completed,
}

impl From<Status> for Look {
    fn from(status: Status) -> Self {
        match status {
            Status::NotStarted | Status::Dropped => Look::NotStarted,
            Status::InProgress => Look::InProgress,
            Status::OnHold => Look::OnHold,
            Status::Completed => Look::Completed,
        }
    }
}

impl From<DerivedStatus> for Look {
    fn from(status: DerivedStatus) -> Self {
        match status {
            DerivedStatus::NotStarted => Look::NotStarted,
            DerivedStatus::InProgress => Look::InProgress,
            DerivedStatus::Completed => Look::Completed,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub node: PlanNode,
    pub depth: u32,
    pub name: String,
    /// First day and the day after the last, a milestone is the one day it lands on
    pub span: Option<(NaiveDate, NaiveDate)>,
    pub look: Look,
    pub unscheduled: Option<UnscheduledReason>,
    /// Starts after the date a constraint asks for
    pub violated: bool,
    pub has_members: bool,
    pub collapsed: bool,
}

pub fn node_name(plan: &Plan, node: PlanNode) -> String {
    match node {
        PlanNode::Task(id) => plan.tasks().get(&id).map(|t| t.name().to_string()),
        PlanNode::Milestone(id) => plan.milestones().get(&id).map(|m| m.name().to_string()),
        PlanNode::TaskGroup(id) => plan.task_groups().get(&id).map(|g| g.name().to_string()),
        PlanNode::PlanBuffer(id) => plan
            .plan_buffers()
            .get(&id)
            .map(|b| format!("Buffer {}d", b.duration())),
    }
    .unwrap_or_else(|| "Missing".to_string())
}

pub fn parent_of(plan: &Plan, node: PlanNode) -> Parent {
    match node {
        PlanNode::Task(id) => plan.tasks().get(&id).map(|t| t.parent()),
        PlanNode::Milestone(id) => plan.milestones().get(&id).map(|m| m.parent()),
        PlanNode::TaskGroup(id) => plan.task_groups().get(&id).map(|g| g.parent()),
        PlanNode::PlanBuffer(id) => plan.plan_buffers().get(&id).map(|b| b.parent()),
    }
    .unwrap_or(Parent::PlanStart)
}

/// The group a node is directly in, found in the plan as the client holds it
pub fn parent_of_in(
    sync: crate::sync::Sync,
    plan: plinko::types::short_id::ShortId,
    node: PlanNode,
) -> Option<Uuid> {
    sync.tenant.with_untracked(|t| {
        let plan = t.as_ref()?.plans().get(&plan)?;
        match parent_of(plan, node) {
            Parent::Node(group) => Some(group),
            Parent::PlanStart => None,
        }
    })
}

/// What a node depends on
pub fn dependencies(plan: &Plan, node: PlanNode) -> Vec<Dependency> {
    match node {
        PlanNode::Task(id) => plan.tasks().get(&id).map(|t| t.dependencies().clone()),
        PlanNode::Milestone(id) => plan.milestones().get(&id).map(|m| m.dependencies().clone()),
        PlanNode::TaskGroup(id) => plan
            .task_groups()
            .get(&id)
            .map(|g| g.dependencies().clone()),
        PlanNode::PlanBuffer(id) => plan
            .plan_buffers()
            .get(&id)
            .map(|b| b.dependencies().clone()),
    }
    .unwrap_or_default()
}

/// Every node of the plan
pub fn nodes(plan: &Plan) -> Vec<PlanNode> {
    let tasks = plan.tasks().keys().map(|id| PlanNode::Task(*id));
    let milestones = plan.milestones().keys().map(|id| PlanNode::Milestone(*id));
    let groups = plan.task_groups().keys().map(|id| PlanNode::TaskGroup(*id));
    let buffers = plan
        .plan_buffers()
        .keys()
        .map(|id| PlanNode::PlanBuffer(*id));
    tasks
        .chain(milestones)
        .chain(groups)
        .chain(buffers)
        .collect()
}

pub fn span_of(
    state: Option<&PlanVolatileState>,
    node: PlanNode,
) -> Option<(NaiveDate, NaiveDate)> {
    let state = state?;
    match node {
        PlanNode::Task(id) => state
            .tasks
            .get(&id)
            .map(|t| (t.date_range.start(), t.date_range.end())),
        PlanNode::Milestone(id) => state.milestones.get(&id).map(|m| (m.date, m.end())),
        PlanNode::TaskGroup(id) => state
            .task_groups
            .get(&id)
            .map(|g| (g.date_range.start(), g.date_range.end())),
        PlanNode::PlanBuffer(id) => state
            .plan_buffers
            .get(&id)
            .map(|b| (b.date_range.start(), b.date_range.end())),
    }
}

pub fn look_of(plan: &Plan, state: Option<&PlanVolatileState>, node: PlanNode) -> Look {
    match node {
        PlanNode::Task(id) => plan
            .tasks()
            .get(&id)
            .map_or(Look::NotStarted, |t| t.persistent_state().status.into()),
        PlanNode::Milestone(id) => state
            .and_then(|s| s.milestones.get(&id))
            .map_or(Look::NotStarted, |m| m.derived_status.into()),
        _ => Look::NotStarted,
    }
}

/// The plan's items in order, groups first with what is in them below, each level by when it
/// starts. What is not scheduled comes last. Members of a collapsed group are left out.
pub fn rows(plan: &Plan, state: Option<&PlanVolatileState>, collapsed: &HashSet<Uuid>) -> Vec<Row> {
    let mut children: HashMap<Parent, Vec<PlanNode>> = HashMap::new();
    for node in nodes(plan) {
        children
            .entry(parent_of(plan, node))
            .or_default()
            .push(node);
    }
    for level in children.values_mut() {
        level.sort_by_key(|n| {
            let start = span_of(state, *n).map(|s| s.0);
            (start.is_none(), start, node_name(plan, *n), n.id())
        });
    }
    let mut rows = Vec::new();
    let context = Context {
        plan,
        state,
        collapsed,
        children: &children,
    };
    context.add_level(Parent::PlanStart, 0, &mut rows);
    rows
}

struct Context<'a> {
    plan: &'a Plan,
    state: Option<&'a PlanVolatileState>,
    collapsed: &'a HashSet<Uuid>,
    children: &'a HashMap<Parent, Vec<PlanNode>>,
}

impl Context<'_> {
    fn add_level(&self, parent: Parent, depth: u32, rows: &mut Vec<Row>) {
        for node in self.children.get(&parent).into_iter().flatten().copied() {
            let group = match node {
                PlanNode::TaskGroup(id) => Some(id),
                _ => None,
            };
            let members = group.is_some_and(|id| self.children.contains_key(&Parent::Node(id)));
            let is_collapsed = group.is_some_and(|id| self.collapsed.contains(&id));
            rows.push(Row {
                node,
                depth,
                name: node_name(self.plan, node),
                span: span_of(self.state, node),
                look: look_of(self.plan, self.state, node),
                unscheduled: self.state.and_then(|s| s.unscheduled.get(&node).copied()),
                violated: self
                    .state
                    .is_some_and(|s| s.constraint_violations.contains_key(&node)),
                has_members: members,
                collapsed: is_collapsed,
            });
            if let (Some(id), false) = (group, is_collapsed) {
                self.add_level(Parent::Node(id), depth + 1, rows);
            }
        }
    }
}

/// What it is that keeps an item off the schedule, in words
pub fn reason(reason: UnscheduledReason) -> &'static str {
    match reason {
        UnscheduledReason::Dropped => "Dropped",
        UnscheduledReason::NoPathToPlanStart => "Nothing connects it to the start of the plan",
        UnscheduledReason::DependsOnUnscheduled => "Everything it depends on is unscheduled",
        UnscheduledReason::NoScheduledMembers => "Nothing in the group could be scheduled",
        UnscheduledReason::NoEligibleWorkers => "Nobody is eligible for the work",
        UnscheduledReason::NoCapacity => "The people available cannot fit the work",
    }
}

#[cfg(test)]
mod tests {
    use plinko::{
        planner::{plan::NewPlan, plan_update::PlanUpdate},
        scheduler::state::{PlanVolatileState, UnscheduledReason},
        types::{
            common::DateRange,
            crud::Crud,
            short_id::ShortId,
            task::{self, NewTask},
            task_group::NewTaskGroup,
        },
    };

    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn plan() -> Plan {
        let new = NewPlan {
            id: ShortId::new(),
            name: "Plan".to_string(),
            start_date: d(1),
        };
        Plan::from(new)
    }

    fn task(plan: &mut Plan, id: u128, name: &str, parent: Parent) -> PlanNode {
        let id = Uuid::from_u128(id);
        let new = NewTask {
            id,
            name: name.to_string(),
            parent,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        PlanNode::Task(id)
    }

    fn scheduled(state: &mut PlanVolatileState, node: PlanNode, from: u32, to: u32) {
        let PlanNode::Task(id) = node else {
            panic!("a task");
        };
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
    fn rows_go_by_start_with_the_unscheduled_last_and_groups_hold_their_members() {
        let mut plan = plan();
        let late = task(&mut plan, 1, "Late", Parent::PlanStart);
        let early = task(&mut plan, 2, "Early", Parent::PlanStart);
        let loose = task(&mut plan, 3, "Loose", Parent::PlanStart);
        let group = Uuid::from_u128(10);
        let new = NewTaskGroup {
            id: group,
            name: "Group".to_string(),
            parent: Parent::PlanStart,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(new)))
            .unwrap();
        let inside = task(&mut plan, 4, "Inside", Parent::Node(group));

        let mut state = PlanVolatileState::default();
        scheduled(&mut state, late, 8, 10);
        scheduled(&mut state, early, 2, 4);
        scheduled(&mut state, inside, 3, 5);
        state
            .unscheduled
            .insert(loose, UnscheduledReason::NoPathToPlanStart);

        let names = |collapsed: &HashSet<Uuid>| -> Vec<(String, u32)> {
            rows(&plan, Some(&state), collapsed)
                .into_iter()
                .map(|r| (r.name, r.depth))
                .collect()
        };
        // the group has no span of its own here so it goes with the unscheduled
        let open = names(&HashSet::new());
        assert_eq!(open[0].0, "Early");
        assert_eq!(open[1].0, "Late");
        let group_at = open.iter().position(|(n, _)| n == "Group").unwrap();
        assert_eq!(open[group_at + 1], ("Inside".to_string(), 1));
        assert_eq!(open.last().unwrap().0, "Loose");

        let collapsed = names(&HashSet::from([group]));
        assert!(collapsed.iter().all(|(n, _)| n != "Inside"));
        let row = rows(&plan, Some(&state), &HashSet::from([group]))
            .into_iter()
            .find(|r| r.name == "Group")
            .unwrap();
        assert!(row.has_members && row.collapsed);
        let loose_row = rows(&plan, Some(&state), &HashSet::new())
            .into_iter()
            .find(|r| r.node == loose)
            .unwrap();
        assert_eq!(
            loose_row.unscheduled,
            Some(UnscheduledReason::NoPathToPlanStart)
        );
        assert_eq!(loose_row.span, None);
    }

    #[test]
    fn items_are_named_by_kind() {
        let mut plan = plan();
        let task = task(&mut plan, 1, "Write", Parent::PlanStart);
        assert_eq!(node_name(&plan, task), "Write");
        assert_eq!(
            node_name(&plan, PlanNode::Task(Uuid::from_u128(99))),
            "Missing"
        );
    }
}
