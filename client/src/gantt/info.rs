//! What the panel says about the item under the pointer: its name as the title, then its details,
//! in the way Plinko 1 worded them.

use plinko::{
    multitenancy::tenant::Tenant,
    planner::plan::Plan,
    scheduler::state::PlanVolatileState,
    types::{
        dependency::PlanNode,
        task::{Status, Worker},
    },
};

use crate::{format, forms, gantt::arrange::Mark, model::node_name};

pub fn status_label(status: Status) -> &'static str {
    match status {
        Status::NotStarted => "Not started",
        Status::InProgress => "In progress",
        Status::OnHold => "On hold",
        Status::Completed => "Complete",
        Status::Dropped => "Dropped",
    }
}

fn last_day(range: (chrono::NaiveDate, chrono::NaiveDate)) -> chrono::NaiveDate {
    range
        .1
        .pred_opt()
        .filter(|d| *d >= range.0)
        .unwrap_or(range.0)
}

fn worker_name(tenant: &Tenant, plan: &Plan, worker: &Worker) -> String {
    let user = |id| {
        tenant
            .users()
            .get(id)
            .map_or("Someone", |u| u.name())
            .to_string()
    };
    match worker {
        Worker::User(id) => user(id),
        Worker::Tag(id) => format!(
            "needs: {}",
            plan.tags().get(id).map_or("a tag", |t| t.name())
        ),
        Worker::UserGroup(id) => {
            format!(
                "anyone in {}",
                tenant.user_groups().get(id).map_or("a group", |g| g.name())
            )
        }
        Worker::Placeholder => "(unassigned)".to_string(),
    }
}

pub fn info(
    tenant: &Tenant,
    plan: &Plan,
    state: Option<&PlanVolatileState>,
    mark: Mark,
) -> Vec<String> {
    let node = mark;
    let mut lines = vec![node_name(plan, node)];
    let violated = state.is_some_and(|s| s.constraint_violations.contains_key(&node));
    match node {
        PlanNode::Task(id) => {
            let Some(task) = plan.tasks().get(&id) else {
                return lines;
            };
            let persistent = task.persistent_state();
            lines.push(format!("Status: {}", status_label(persistent.status)));
            let range = state
                .and_then(|s| s.tasks.get(&id))
                .map(|t| (t.date_range.start(), t.date_range.end()));
            match (persistent.actual_start_date, range) {
                (Some(started), _) => lines.push(format!("Started: {}", format::day(started))),
                (None, Some(range)) => lines.push(format!("Scheduled: {}", format::day(range.0))),
                _ => {}
            }
            if let Some(range) = range {
                lines.push(format!("Ends: {}", format::day(last_day(range))));
            }
            let mut workers: Vec<String> = task
                .allocations()
                .keys()
                .map(|w| worker_name(tenant, plan, w))
                .collect();
            workers.sort();
            if !workers.is_empty() {
                lines.push(format!("Workers: {}", workers.join(", ")));
            }
        }
        PlanNode::Milestone(id) => {
            lines.push("Milestone".to_string());
            if let Some(m) = state.and_then(|s| s.milestones.get(&id)) {
                lines.push(format!("Scheduled: {}", format::day(m.date)));
            }
        }
        PlanNode::PlanBuffer(id) => {
            lines.push("Buffer".to_string());
            if let Some(b) = state.and_then(|s| s.plan_buffers.get(&id)) {
                let range = (b.date_range.start(), b.date_range.end());
                lines.push(format!(
                    "From {} to {}",
                    format::day(range.0),
                    format::day(last_day(range))
                ));
            }
        }
        PlanNode::TaskGroup(id) => {
            lines.push("Task group".to_string());
            if let Some(g) = state.and_then(|s| s.task_groups.get(&id)) {
                let range = (g.date_range.start(), g.date_range.end());
                lines.push(format!(
                    "From {} to {}",
                    format::day(range.0),
                    format::day(last_day(range))
                ));
            }
        }
    }
    if violated {
        lines.push("⚠ Constraint violated".to_string());
    }
    lines
}

/// What the small warning beside an item says
pub fn violation(state: Option<&PlanVolatileState>, mark: Mark) -> Vec<String> {
    let Some(v) = state.and_then(|s| s.constraint_violations.get(&mark)) else {
        return Vec::new();
    };
    let (kind, date) = forms::constraint_parts(v.constraint);
    let title = match kind {
        "fixed" => "Fixed date violated",
        "latest" => "Latest date exceeded",
        _ => "Earliest date not met",
    };
    let required = date.map(format::iso).unwrap_or_default();
    let mut lines = vec![
        title.to_string(),
        format!("Required: {required}"),
        format!("Scheduled: {}", format::iso(v.scheduled)),
    ];
    if let Some(earliest) = v.earliest.filter(|_| v.avoidable() == 0) {
        let earliest = format::iso(earliest);
        lines.push(format!("Cannot be met: the work before it ends {earliest}"));
    }
    lines
}
