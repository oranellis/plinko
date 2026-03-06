use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// How the scheduler should treat a pinned date on a task or milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// Must start on exactly this date, regardless of dependencies or workload.
    Fixed,
    /// Cannot start before this date; dependencies may still push it later.
    Earliest,
    /// Must start no later than this date; the scheduler will flag a violation
    /// if dependencies or workload make this impossible.
    Latest,
}

/// A scheduling constraint pinning a task or milestone to a specific date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateConstraint {
    pub date: NaiveDate,
    pub kind: ConstraintKind,
}

impl DateConstraint {
    pub fn fixed(date: NaiveDate) -> Self {
        Self { date, kind: ConstraintKind::Fixed }
    }

    pub fn earliest(date: NaiveDate) -> Self {
        Self { date, kind: ConstraintKind::Earliest }
    }

    pub fn latest(date: NaiveDate) -> Self {
        Self { date, kind: ConstraintKind::Latest }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn fixed_constructor() {
        let c = DateConstraint::fixed(date(2026, 3, 9));
        assert_eq!(c.date, date(2026, 3, 9));
        assert_eq!(c.kind, ConstraintKind::Fixed);
    }

    #[test]
    fn earliest_constructor() {
        let c = DateConstraint::earliest(date(2026, 3, 9));
        assert_eq!(c.kind, ConstraintKind::Earliest);
    }

    #[test]
    fn latest_constructor() {
        let c = DateConstraint::latest(date(2026, 3, 9));
        assert_eq!(c.kind, ConstraintKind::Latest);
    }

    #[test]
    fn constraint_equality() {
        let d = date(2026, 3, 9);
        assert_eq!(DateConstraint::fixed(d), DateConstraint::fixed(d));
        assert_ne!(DateConstraint::fixed(d), DateConstraint::earliest(d));
    }
}
