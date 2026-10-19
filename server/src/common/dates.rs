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
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TimeConstraint {
    #[default]
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}

/// Where a start ends up under a time constraint
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstrainedStart {
    pub start: NaiveDate,
    /// The constraint asks for something its dependencies do not allow
    pub violated: bool,
}
// }}}

// impl TimeConstraint {{{
impl TimeConstraint {
    /// The start a task or milestone gets when its dependencies allow `earliest`. The constraint
    /// date is the item's start. A fixed or latest date that is already passed cannot be met, so
    /// the item starts at `earliest` and the constraint is violated.
    pub fn apply(self, earliest: NaiveDate) -> ConstrainedStart {
        let (start, violated) = match self {
            TimeConstraint::None => (earliest, false),
            TimeConstraint::Earliest(date) => (earliest.max(date), false),
            TimeConstraint::Latest(date) => (earliest, earliest > date),
            TimeConstraint::Fixed(date) => (earliest.max(date), earliest > date),
        };
        ConstrainedStart { start, violated }
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn applied(constraint: TimeConstraint, earliest: u32) -> (NaiveDate, bool) {
        let result = constraint.apply(d(earliest));
        (result.start, result.violated)
    }

    #[test]
    fn constraints_apply_to_the_start() {
        use TimeConstraint::*;
        assert_eq!(applied(None, 5), (d(5), false));
        assert_eq!(applied(Earliest(d(8)), 5), (d(8), false));
        assert_eq!(applied(Earliest(d(3)), 5), (d(5), false));
        assert_eq!(applied(Latest(d(8)), 5), (d(5), false));
        assert_eq!(applied(Latest(d(3)), 5), (d(5), true));
        assert_eq!(applied(Fixed(d(8)), 5), (d(8), false));
        assert_eq!(applied(Fixed(d(5)), 5), (d(5), false));
        assert_eq!(applied(Fixed(d(3)), 5), (d(5), true));
    }
}
// }}}
