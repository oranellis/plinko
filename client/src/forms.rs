//! Choices in a form as text, and back. A select holds a string, so everything a select picks
//! has a way to be written down and read again.

// modules {{{
use chrono::NaiveDate;
use plinko::{
    common::dates::{NodeConstraint, NodeEnd, TimeConstraint},
    planner::items::task::Worker,
};
use uuid::Uuid;
// }}}

// helpers {{{
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

/// The constraint a task or group form has, which is on the first day when there is none
pub fn node_constraint(kind: &str, day: NaiveDate, end: NodeEnd) -> NodeConstraint {
    match constraint_from(kind, day) {
        TimeConstraint::None => NodeConstraint::default(),
        constraint => NodeConstraint { constraint, end },
    }
}

pub fn end_value(end: NodeEnd) -> &'static str {
    match end {
        NodeEnd::Start => "start",
        NodeEnd::End => "end",
    }
}

pub fn parse_end(value: &str) -> NodeEnd {
    match value {
        "end" => NodeEnd::End,
        _ => NodeEnd::Start,
    }
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
        for worker in [
            Worker::User(id()),
            Worker::Tag(id()),
            Worker::UserGroup(id()),
            Worker::Placeholder,
        ] {
            assert_eq!(parse_worker(&worker_value(worker)), Some(worker));
        }
    }

    #[test]
    fn nonsense_is_not_read() {
        assert_eq!(parse_worker(""), None);
        assert_eq!(parse_worker("user:not-an-id"), None);
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

    #[test]
    fn a_form_with_no_constraint_forgets_which_day_it_was_for() {
        let day = NaiveDate::from_ymd_opt(2024, 3, 4).unwrap();
        let on_end = node_constraint("latest", day, parse_end(end_value(NodeEnd::End)));
        assert_eq!(on_end.end, NodeEnd::End);
        assert_eq!(on_end.constraint, TimeConstraint::Latest(day));
        assert_eq!(
            node_constraint("none", day, NodeEnd::End),
            NodeConstraint::default()
        );
    }
}
// }}}
