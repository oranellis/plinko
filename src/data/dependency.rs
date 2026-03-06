use serde::{Deserialize, Serialize};
use crate::data::ids::DependencyId;

/// A dependency edge with an optional lag.
///
/// `lag_days` is in working days:
///   - positive = delay (successor cannot start until N days after predecessor completes)
///   - negative = lead / overlap (successor can start N days *before* predecessor completes)
///   - zero (default) = start immediately after predecessor completes
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub id: DependencyId,
    pub lag_days: f32,
}

impl Dependency {
    /// No lag — successor starts as soon as the predecessor completes.
    pub fn new(id: DependencyId) -> Self {
        Self { id, lag_days: 0.0 }
    }

    /// Positive lag: delay start by `days` working days after predecessor completes.
    pub fn with_lag(id: DependencyId, days: f32) -> Self {
        Self { id, lag_days: days }
    }

    /// Negative lag (lead): successor may start `days` working days *before*
    /// the predecessor completes.
    pub fn with_lead(id: DependencyId, days: f32) -> Self {
        Self { id, lag_days: -days.abs() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ids::TaskId;

    fn dep_id() -> DependencyId {
        DependencyId::Task(TaskId::new())
    }

    #[test]
    fn new_has_zero_lag() {
        let d = Dependency::new(dep_id());
        assert_eq!(d.lag_days, 0.0);
    }

    #[test]
    fn with_lag_stores_positive_days() {
        let d = Dependency::with_lag(dep_id(), 3.0);
        assert_eq!(d.lag_days, 3.0);
    }

    #[test]
    fn with_lead_stores_negative_days() {
        let d = Dependency::with_lead(dep_id(), 2.0);
        assert_eq!(d.lag_days, -2.0);
    }

    #[test]
    fn with_lead_normalises_negative_input() {
        // Passing a negative value to with_lead should still produce a negative lag.
        let d = Dependency::with_lead(dep_id(), -2.0);
        assert_eq!(d.lag_days, -2.0);
    }

    #[test]
    fn plan_start_dependency_with_lag() {
        let d = Dependency::with_lag(DependencyId::PlanStart, 5.0);
        assert_eq!(d.id, DependencyId::PlanStart);
        assert_eq!(d.lag_days, 5.0);
    }
}
