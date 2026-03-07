//! The [`Task`] type and its [`TaskStatus`] enum.

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::UserId;
use crate::data::ids::TaskId;
use crate::data::user::User;

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    OnHold,
    Complete,
    Dropped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub dependencies: Vec<Dependency>,
    /// Per-user workload in days. Keys are the assigned users.
    /// Use `Task::with_equal_split` to construct with automatic splitting.
    pub workload_days: HashMap<UserId, f32>,
    /// Optional scheduling constraint. When set, the auto-scheduler respects
    /// the pinned date according to the constraint kind (fixed, earliest, latest).
    pub constraint: Option<DateConstraint>,
    /// Tags a user must possess all of to be eligible to work on this task.
    /// An empty set means any user can be assigned.
    pub required_tags: HashSet<String>,
    /// Calendar span of the task in working days, independent of workload effort.
    /// Set explicitly by the user; 0.0 means "not yet set / use workload as default".
    pub duration_days: f32,
}

impl Task {
    /// Create a task with no users assigned yet.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::NotStarted,
            dependencies: Vec::new(),
            workload_days: HashMap::new(),
            constraint: None,
            required_tags: HashSet::new(),
            duration_days: 0.0,
        }
    }

    /// Create a task with `total_days` split equally across the given users.
    pub fn with_equal_split(
        name: impl Into<String>,
        description: impl Into<String>,
        total_days: f32,
        users: &[UserId],
    ) -> Self {
        let per_user = if users.is_empty() { 0.0 } else { total_days / users.len() as f32 };
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::NotStarted,
            dependencies: Vec::new(),
            workload_days: users.iter().map(|&u| (u, per_user)).collect(),
            constraint: None,
            required_tags: HashSet::new(),
            duration_days: 0.0,
        }
    }

    /// Total workload across all assigned users.
    pub fn total_workload_days(&self) -> f32 {
        self.workload_days.values().sum()
    }

    /// Assigned users (derived from workload_days keys — no separate Vec to keep in sync).
    pub fn assigned_users(&self) -> impl Iterator<Item = &UserId> {
        self.workload_days.keys()
    }

    pub fn require_tag(&mut self, tag: impl Into<String>) {
        self.required_tags.insert(tag.into());
    }

    pub fn remove_required_tag(&mut self, tag: &str) {
        self.required_tags.remove(tag);
    }

    /// Set the calendar duration in days. Negative values are clamped to 0.
    pub fn with_duration(mut self, days: f32) -> Self {
        self.duration_days = days.max(0.0);
        self
    }

    /// Effective calendar duration: the explicit `duration_days` if set (> 0),
    /// otherwise defaults to `(total_workload_days / num_assigned_users) * 2.0`.
    /// This assumes each person works at 50% focus by default — 2 days of workload
    /// takes 4 calendar days for 1 person, or 2 calendar days for 2 people, etc.
    /// Returns 0.0 if no users are assigned and duration is unset.
    pub fn effective_duration_days(&self) -> f32 {
        if self.duration_days > 0.0 {
            return self.duration_days;
        }
        let n = self.workload_days.len();
        if n == 0 {
            return 0.0;
        }
        (self.total_workload_days() / n as f32) * 2.0
    }

    /// Returns true if the user holds every tag required by this task.
    /// A task with no required tags accepts any user.
    pub fn is_user_eligible(&self, user: &User) -> bool {
        self.required_tags.is_subset(&user.tags)
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
    fn new_task_has_correct_defaults() {
        let t = Task::new("Design", "Create wireframes");
        assert_eq!(t.name, "Design");
        assert_eq!(t.description, "Create wireframes");
        assert_eq!(t.status, TaskStatus::NotStarted);
        assert!(t.dependencies.is_empty());
        assert!(t.workload_days.is_empty());
        assert!(t.constraint.is_none());
    }

    #[test]
    fn new_tasks_have_unique_ids() {
        let a = Task::new("A", "");
        let b = Task::new("B", "");
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn equal_split_divides_workload() {
        let users = vec![UserId::new(), UserId::new(), UserId::new()];
        let t = Task::with_equal_split("Build", "", 9.0, &users);
        for u in &users {
            assert!((t.workload_days[u] - 3.0).abs() < f32::EPSILON);
        }
        assert!((t.total_workload_days() - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn equal_split_with_no_users_is_zero() {
        let t = Task::with_equal_split("Build", "", 10.0, &[]);
        assert!(t.workload_days.is_empty());
        assert_eq!(t.total_workload_days(), 0.0);
    }

    #[test]
    fn assigned_users_matches_workload_keys() {
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("X", "", 4.0, &users);
        let mut assigned: Vec<_> = t.assigned_users().copied().collect();
        assigned.sort_by_key(|u| u.0);
        let mut expected = users.clone();
        expected.sort_by_key(|u| u.0);
        assert_eq!(assigned, expected);
    }

    #[test]
    fn total_workload_sums_all_users() {
        let u1 = UserId::new();
        let u2 = UserId::new();
        let mut t = Task::new("X", "");
        t.workload_days.insert(u1, 3.0);
        t.workload_days.insert(u2, 5.0);
        assert!((t.total_workload_days() - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn constraint_can_be_set_and_cleared() {
        let mut t = Task::new("T", "");
        t.constraint = Some(DateConstraint::fixed(date(2026, 3, 9)));
        assert_eq!(t.constraint.unwrap().kind, ConstraintKind::Fixed);
        assert_eq!(t.constraint.unwrap().date, date(2026, 3, 9));
        t.constraint = None;
        assert!(t.constraint.is_none());
    }

    #[test]
    fn all_constraint_kinds_on_task() {
        let d = date(2026, 6, 1);
        let mut t = Task::new("T", "");

        t.constraint = Some(DateConstraint::earliest(d));
        assert_eq!(t.constraint.unwrap().kind, ConstraintKind::Earliest);

        t.constraint = Some(DateConstraint::latest(d));
        assert_eq!(t.constraint.unwrap().kind, ConstraintKind::Latest);
    }

    #[test]
    fn equal_split_has_no_constraint_by_default() {
        let users = vec![UserId::new()];
        let t = Task::with_equal_split("T", "", 5.0, &users);
        assert!(t.constraint.is_none());
    }

    // ── Affinity / required tags ──────────────────────────────────────────────

    #[test]
    fn new_task_has_no_required_tags() {
        let t = Task::new("T", "");
        assert!(t.required_tags.is_empty());
    }

    #[test]
    fn task_with_no_required_tags_accepts_any_user() {
        let t = Task::new("T", "");
        let u = User::new("Alice"); // no tags
        assert!(t.is_user_eligible(&u));
    }

    #[test]
    fn user_with_all_required_tags_is_eligible() {
        let mut t = Task::new("T", "");
        t.require_tag("rust");
        t.require_tag("skia");
        let u = User::new("Alice").with_tag("rust").with_tag("skia").with_tag("extra");
        assert!(t.is_user_eligible(&u));
    }

    #[test]
    fn user_missing_one_tag_is_not_eligible() {
        let mut t = Task::new("T", "");
        t.require_tag("rust");
        t.require_tag("skia");
        let u = User::new("Alice").with_tag("rust"); // missing "skia"
        assert!(!t.is_user_eligible(&u));
    }

    #[test]
    fn user_with_no_tags_is_not_eligible_when_tags_required() {
        let mut t = Task::new("T", "");
        t.require_tag("rust");
        let u = User::new("Alice");
        assert!(!t.is_user_eligible(&u));
    }

    #[test]
    fn removing_required_tag_makes_user_eligible() {
        let mut t = Task::new("T", "");
        t.require_tag("rust");
        t.require_tag("skia");
        let u = User::new("Alice").with_tag("rust");
        assert!(!t.is_user_eligible(&u));
        t.remove_required_tag("skia");
        assert!(t.is_user_eligible(&u));
    }

    #[test]
    fn duplicate_required_tags_are_deduplicated() {
        let mut t = Task::new("T", "");
        t.require_tag("rust");
        t.require_tag("rust");
        assert_eq!(t.required_tags.len(), 1);
    }

    // ── duration_days ─────────────────────────────────────────────────────────

    #[test]
    fn default_duration_days_is_zero() {
        let t = Task::new("T", "");
        assert_eq!(t.duration_days, 0.0);
    }

    #[test]
    fn with_duration_stores_value() {
        let t = Task::new("T", "").with_duration(5.0);
        assert_eq!(t.duration_days, 5.0);
    }

    #[test]
    fn with_duration_clamps_negative_to_zero() {
        let t = Task::new("T", "").with_duration(-1.0);
        assert_eq!(t.duration_days, 0.0);
    }

    #[test]
    fn effective_duration_returns_explicit_when_set() {
        let u = UserId::new();
        let t = Task::with_equal_split("T", "", 2.0, &[u]).with_duration(7.0);
        assert_eq!(t.effective_duration_days(), 7.0);
    }

    #[test]
    fn effective_duration_no_users_no_duration_is_zero() {
        let t = Task::new("T", "");
        assert_eq!(t.effective_duration_days(), 0.0);
    }

    #[test]
    fn effective_duration_one_user_two_days_workload() {
        let u = UserId::new();
        let t = Task::with_equal_split("T", "", 2.0, &[u]);
        assert!((t.effective_duration_days() - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_duration_two_users_two_days_each() {
        // total = 4, n = 2 → average = 2.0 → effective = 4.0
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 4.0, &users);
        assert!((t.effective_duration_days() - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_duration_two_users_one_day_each() {
        // total = 2, n = 2 → average = 1.0 → effective = 2.0
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 2.0, &users);
        assert!((t.effective_duration_days() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn explicit_duration_overrides_formula() {
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 4.0, &users).with_duration(3.0);
        assert_eq!(t.effective_duration_days(), 3.0);
    }
}
