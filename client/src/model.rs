//! A plan as rows to show: the tree of groups and items in order, with where each is scheduled.

use std::collections::HashSet;

use chrono::NaiveDate;
use plinko::{
    planner::plan::Plan,
    scheduler::state::{PlanVolatileState, UnscheduledReason},
    types::{dependency::PlanNode, milestone::DerivedStatus, parent::Parent, task::Status},
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

fn parent_of(plan: &Plan, node: PlanNode) -> Parent {
    match node {
        PlanNode::Task(id) => plan.tasks().get(&id).map(|t| t.parent()),
        PlanNode::Milestone(id) => plan.milestones().get(&id).map(|m| m.parent()),
        PlanNode::TaskGroup(id) => plan.task_groups().get(&id).map(|g| g.parent()),
        PlanNode::PlanBuffer(id) => plan.plan_buffers().get(&id).map(|b| b.parent()),
    }
    .unwrap_or(Parent::PlanStart)
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

fn span_of(state: Option<&PlanVolatileState>, node: PlanNode) -> Option<(NaiveDate, NaiveDate)> {
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

fn look_of(plan: &Plan, state: Option<&PlanVolatileState>, node: PlanNode) -> Look {
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
    let mut rows = Vec::new();
    add_level(plan, state, collapsed, Parent::PlanStart, 0, &mut rows);
    rows
}

fn add_level(
    plan: &Plan,
    state: Option<&PlanVolatileState>,
    collapsed: &HashSet<Uuid>,
    parent: Parent,
    depth: u32,
    rows: &mut Vec<Row>,
) {
    let mut level: Vec<PlanNode> = nodes(plan)
        .into_iter()
        .filter(|n| parent_of(plan, *n) == parent)
        .collect();
    level.sort_by_key(|n| {
        (
            span_of(state, *n).map(|s| s.0).is_none(),
            span_of(state, *n).map(|s| s.0),
            node_name(plan, *n),
            n.id(),
        )
    });
    for node in level {
        let has_members = matches!(node, PlanNode::TaskGroup(id) if nodes(plan).iter().any(|n| parent_of(plan, *n) == Parent::Node(id)));
        let is_collapsed = matches!(node, PlanNode::TaskGroup(id) if collapsed.contains(&id));
        rows.push(Row {
            node,
            depth,
            name: node_name(plan, node),
            span: span_of(state, node),
            look: look_of(plan, state, node),
            unscheduled: state.and_then(|s| s.unscheduled.get(&node).copied()),
            violated: state.is_some_and(|s| s.constraint_violations.contains_key(&node)),
            has_members,
            collapsed: is_collapsed,
        });
        if let (PlanNode::TaskGroup(id), false) = (node, is_collapsed) {
            add_level(plan, state, collapsed, Parent::Node(id), depth + 1, rows);
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
