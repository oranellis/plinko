// modules {{{
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
// }}}

/// The days from `start` up to but not including `end`, so a zero length range has equal ends
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

// impl DateRange {{{
impl DateRange {
    /// `None` if `end` is before `start`
    pub fn new(start: NaiveDate, end: NaiveDate) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn start(&self) -> NaiveDate {
        self.start
    }

    pub fn end(&self) -> NaiveDate {
        self.end
    }
}
// }}}

// DateRange supporting types {{{
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeConstraint {
    #[default]
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}

/// Which end of a task or group a time constraint is about
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeEnd {
    /// Its first day
    #[default]
    Start,
    /// Its last day
    End,
}

/// A time constraint on the first or the last day of a task or group. A milestone is a single
/// day, so it has a plain `TimeConstraint`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeConstraint {
    pub constraint: TimeConstraint,
    pub end: NodeEnd,
}
// }}}

// impl TimeConstraint {{{
impl TimeConstraint {
    /// The day the constrained day may not be before, and the day it may not be after
    pub fn bounds(self) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self {
            TimeConstraint::None => (None, None),
            TimeConstraint::Earliest(date) => (Some(date), None),
            TimeConstraint::Latest(date) => (None, Some(date)),
            TimeConstraint::Fixed(date) => (Some(date), Some(date)),
        }
    }
}
// }}}

// impl NodeConstraint {{{
impl NodeConstraint {
    /// The bounds if the constraint is about `end`, none otherwise
    pub fn bounds_on(self, end: NodeEnd) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self.end == end {
            true => self.constraint.bounds(),
            false => (None, None),
        }
    }
}

impl From<TimeConstraint> for NodeConstraint {
    /// A constraint on the start
    fn from(constraint: TimeConstraint) -> Self {
        Self {
            constraint,
            end: NodeEnd::Start,
        }
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_given_for_the_end_a_constraint_is_about() {
        let d = |day| NaiveDate::from_ymd_opt(2024, 1, day).unwrap();
        assert_eq!(TimeConstraint::None.bounds(), (None, None));
        assert_eq!(TimeConstraint::Earliest(d(3)).bounds(), (Some(d(3)), None));
        assert_eq!(TimeConstraint::Latest(d(3)).bounds(), (None, Some(d(3))));
        assert_eq!(
            TimeConstraint::Fixed(d(3)).bounds(),
            (Some(d(3)), Some(d(3)))
        );
        let on_end = NodeConstraint {
            constraint: TimeConstraint::Latest(d(3)),
            end: NodeEnd::End,
        };
        assert_eq!(on_end.bounds_on(NodeEnd::End), (None, Some(d(3))));
        assert_eq!(on_end.bounds_on(NodeEnd::Start), (None, None));
        let on_start: NodeConstraint = TimeConstraint::Fixed(d(3)).into();
        assert_eq!(on_start.bounds_on(NodeEnd::Start), (Some(d(3)), Some(d(3))));
    }
}
// }}}
