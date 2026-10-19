//! Choices in a form as text, and back. A select holds a string, so everything a select picks
//! has a way to be written down and read again.

// modules {{{
use chrono::NaiveDate;
use plinko::types::{
    common::TimeConstraint,
    dependency::{DependencyNode, PlanNode},
    parent::Parent,
    task::{Status, Worker},
};
use uuid::Uuid;
// }}}

// helpers {{{
pub fn node_value(node: DependencyNode) -> String {
    match node {
        DependencyNode::ParentStart => "start".to_string(),
        DependencyNode::Node(PlanNode::Task(id)) => format!("task:{id}"),
        DependencyNode::Node(PlanNode::Milestone(id)) => format!("milestone:{id}"),
        DependencyNode::Node(PlanNode::PlanBuffer(id)) => format!("buffer:{id}"),
        DependencyNode::Node(PlanNode::PlanDelay(id)) => format!("delay:{id}"),
        DependencyNode::Node(PlanNode::TaskGroup(id)) => format!("group:{id}"),
    }
}

pub fn parse_node(text: &str) -> Option<DependencyNode> {
    if text == "start" {
        return Some(DependencyNode::ParentStart);
    }
    let (kind, id) = text.split_once(':')?;
    let id = Uuid::parse_str(id).ok()?;
    let node = match kind {
        "task" => PlanNode::Task(id),
        "milestone" => PlanNode::Milestone(id),
        "buffer" => PlanNode::PlanBuffer(id),
        "delay" => PlanNode::PlanDelay(id),
        "group" => PlanNode::TaskGroup(id),
        _ => return None,
    };
    Some(DependencyNode::Node(node))
}

pub fn worker_value(worker: Worker) -> String {
    match worker {
        Worker::User(id) => format!("user:{id}"),
        Worker::Tag(id) => format!("tag:{id}"),
        Worker::UserGroup(id) => format!("group:{id}"),
        Worker::Placeholder => "placeholder".to_string(),
    }
}

pub fn parse_worker(text: &str) -> Option<Worker> {
    if text == "placeholder" {
        return Some(Worker::Placeholder);
    }
    let (kind, id) = text.split_once(':')?;
    let id = Uuid::parse_str(id).ok()?;
    match kind {
        "user" => Some(Worker::User(id)),
        "tag" => Some(Worker::Tag(id)),
        "group" => Some(Worker::UserGroup(id)),
        _ => None,
    }
}

pub fn parent_value(parent: Parent) -> String {
    match parent {
        Parent::Plan => "plan".to_string(),
        Parent::Node(id) => format!("group:{id}"),
    }
}

pub fn parse_parent(text: &str) -> Option<Parent> {
    match text {
        "plan" => Some(Parent::Plan),
        _ => text
            .strip_prefix("group:")
            .and_then(|id| Uuid::parse_str(id).ok())
            .map(Parent::Node),
    }
}

/// The kind of constraint a form has, and the day for the kinds that have one
pub fn constraint_parts(constraint: TimeConstraint) -> (&'static str, Option<NaiveDate>) {
    match constraint {
        TimeConstraint::None => ("none", None),
        TimeConstraint::Earliest(day) => ("earliest", Some(day)),
        TimeConstraint::Latest(day) => ("latest", Some(day)),
        TimeConstraint::Fixed(day) => ("fixed", Some(day)),
    }
}

pub fn constraint_from(kind: &str, day: NaiveDate) -> TimeConstraint {
    match kind {
        "earliest" => TimeConstraint::Earliest(day),
        "latest" => TimeConstraint::Latest(day),
        "fixed" => TimeConstraint::Fixed(day),
        _ => TimeConstraint::None,
    }
}

pub fn status_value(status: Status) -> &'static str {
    match status {
        Status::NotStarted => "not-started",
        Status::InProgress => "in-progress",
        Status::OnHold => "on-hold",
        Status::Completed => "completed",
        Status::Dropped => "dropped",
    }
}

pub fn parse_status(text: &str) -> Option<Status> {
    [
        Status::NotStarted,
        Status::InProgress,
        Status::OnHold,
        Status::Completed,
        Status::Dropped,
    ]
    .into_iter()
    .find(|s| status_value(*s) == text)
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::from_u128(0xabcdef)
    }

    #[test]
    fn what_a_select_picks_is_read_back() {
        for node in [
            DependencyNode::ParentStart,
            DependencyNode::Node(PlanNode::Task(id())),
            DependencyNode::Node(PlanNode::Milestone(id())),
            DependencyNode::Node(PlanNode::PlanBuffer(id())),
            DependencyNode::Node(PlanNode::TaskGroup(id())),
        ] {
            assert_eq!(parse_node(&node_value(node)), Some(node));
        }
        for worker in [
            Worker::User(id()),
            Worker::Tag(id()),
            Worker::UserGroup(id()),
            Worker::Placeholder,
        ] {
            assert_eq!(parse_worker(&worker_value(worker)), Some(worker));
        }
        for parent in [Parent::Plan, Parent::Node(id())] {
            assert_eq!(parse_parent(&parent_value(parent)), Some(parent));
        }
        for status in [
            Status::NotStarted,
            Status::InProgress,
            Status::OnHold,
            Status::Completed,
            Status::Dropped,
        ] {
            assert_eq!(parse_status(status_value(status)), Some(status));
        }
    }

    #[test]
    fn nonsense_is_not_read() {
        assert_eq!(parse_node("task:not-an-id"), None);
        assert_eq!(parse_node("sprint:abc"), None);
        assert_eq!(parse_worker(""), None);
        assert_eq!(parse_parent("group:"), None);
        assert_eq!(parse_status("done"), None);
    }

    #[test]
    fn constraints_split_into_a_kind_and_a_day() {
        let day = NaiveDate::from_ymd_opt(2024, 3, 4).unwrap();
        for constraint in [
            TimeConstraint::None,
            TimeConstraint::Earliest(day),
            TimeConstraint::Latest(day),
            TimeConstraint::Fixed(day),
        ] {
            let (kind, picked) = constraint_parts(constraint);
            assert_eq!(constraint_from(kind, picked.unwrap_or(day)), constraint);
        }
    }
}
// }}}
