//! The [`Milestone`] type — a zero-duration schedule anchor with optional dependencies.

use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::MilestoneId;
use serde::{Deserialize, Serialize};

/// A named point in time that other tasks and milestones can depend on.
///
/// Milestones have no workload — they are pure scheduling anchors.  Like
/// tasks, they support an optional [`DateConstraint`] and a list of
/// predecessor [`Dependency`] edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: MilestoneId,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    /// Optional scheduling constraint. When set, the auto-scheduler respects
    /// the pinned date according to the constraint kind (fixed, earliest, latest).
    pub constraint: Option<DateConstraint>,
}

impl Milestone {
    /// Creates a milestone with no dependencies and no constraint.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: MilestoneId::new(),
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            constraint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::data::constraint::{ConstraintKind, DateConstraint};
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn new_milestone_has_correct_defaults() {
        let m = Milestone::new("Launch", "Public release");
        assert_eq!(m.name, "Launch");
        assert_eq!(m.description, "Public release");
        assert!(m.dependencies.is_empty());
        assert!(m.constraint.is_none());
    }

    #[test]
    fn new_milestones_have_unique_ids() {
        let a = Milestone::new("A", "");
        let b = Milestone::new("B", "");
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn constraint_can_be_set_and_cleared() {
        let mut m = Milestone::new("M", "");
        m.constraint = Some(DateConstraint::latest(date(2026, 12, 31)));
        assert_eq!(m.constraint.unwrap().kind, ConstraintKind::Latest);
        assert_eq!(m.constraint.unwrap().date, date(2026, 12, 31));
        m.constraint = None;
        assert!(m.constraint.is_none());
    }

    #[test]
    fn all_constraint_kinds_on_milestone() {
        let d = date(2026, 6, 1);
        let mut m = Milestone::new("M", "");

        m.constraint = Some(DateConstraint::fixed(d));
        assert_eq!(m.constraint.unwrap().kind, ConstraintKind::Fixed);

        m.constraint = Some(DateConstraint::earliest(d));
        assert_eq!(m.constraint.unwrap().kind, ConstraintKind::Earliest);
    }
}
