// modules{{{
use std::hash::Hash;

use chrono::{Duration, NaiveDate};
use uuid::Uuid;
// }}}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    pub node: DependencyNode,
    /// Calendar days (not working days) after the node, negative to start before it
    pub lag: i32,
}

impl Dependency {
    /// The day a task, group or buffer can start when the node it depends on ends on `end`
    /// (exclusive, so the day after its last day). A milestone counts as the one day it lands on.
    /// `ParentStart` is a date and also uses this, for milestones too.
    pub fn start_after(&self, end: NaiveDate) -> NaiveDate {
        end + Duration::days(i64::from(self.lag))
    }

    /// The day a milestone lands when the node it depends on ends on `end` (exclusive): the last
    /// day of that node, so milestones are zero time events at the trailing edge of the work they
    /// depend on, and share a day with milestones they depend on.
    pub fn milestone_date_after(&self, end: NaiveDate) -> NaiveDate {
        self.start_after(end) - Duration::days(1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DependencyNode {
    /// The start of the enclosing task group, or the plan start for top level items
    #[default]
    ParentStart,
    Node(PlanNode),
}

impl PlanNode {
    pub fn id(&self) -> Uuid {
        match *self {
            PlanNode::Task(id)
            | PlanNode::Milestone(id)
            | PlanNode::PlanBuffer(id)
            | PlanNode::TaskGroup(id) => id,
        }
    }
}

/// A schedulable node. A task group is a single node that ends when all of its members end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanNode {
    Task(Uuid),
    Milestone(Uuid),
    PlanBuffer(Uuid),
    TaskGroup(Uuid),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lag_is_calendar_days_and_may_lead() {
        let end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(); // a Friday
        let after = |lag| Dependency {
            node: DependencyNode::ParentStart,
            lag,
        };
        assert_eq!(after(0).start_after(end), end);
        assert_eq!(
            after(3).start_after(end),
            NaiveDate::from_ymd_opt(2024, 1, 8).unwrap()
        );
        assert_eq!(
            after(-2).start_after(end),
            NaiveDate::from_ymd_opt(2024, 1, 3).unwrap()
        );
    }
}
