//! Form choices to and from the strings a select holds.

// modules {{{
use chrono::NaiveDate;
use plinko::{common::dates::TimeConstraint, planner::items::task::Worker};
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

pub fn constraint_parts(constraint: TimeConstraint) -> (&'static str, Option<NaiveDate>) {
    match constraint {
        TimeConstraint::None => ("none", None),
        TimeConstraint::Earliest(day) => ("earliest", Some(day)),
        TimeConstraint::Latest(day) => ("latest", Some(day)),
        TimeConstraint::Fixed(day) => ("fixed", Some(day)),
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
}
// }}}
