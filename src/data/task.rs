//! The [`Task`] type and its [`TaskStatus`] and [`WorkerSlot`] types.

use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::TaskId;
use crate::data::ids::UserId;
use crate::data::user::User;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    OnHold,
    Complete,
    Dropped,
}

/// A worker assignment on a task — either a named person or an open role.
///
/// `Specific` pins a known user to the task for `workload_days` of effort.
/// `Placeholder` describes an unfilled role: any user holding all `required_tags`
/// can satisfy it.  The scheduler uses placeholders to check completability and
/// to assign the best-fit user when computing the optimised plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerSlot {
    /// A specific team member assigned to this task.
    Specific { user_id: UserId, workload_days: f32 },
    /// An open role filled by whoever holds all the required tags.
    Placeholder { required_tags: HashSet<String>, workload_days: f32 },
}

impl WorkerSlot {
    /// Workload effort in days for this slot.
    pub fn workload_days(&self) -> f32 {
        match self {
            WorkerSlot::Specific { workload_days, .. }
            | WorkerSlot::Placeholder { workload_days, .. } => *workload_days,
        }
    }

    /// Returns `true` if `user` is eligible to fill this slot.
    /// A `Specific` slot only matches the pinned user; a `Placeholder` matches
    /// any user who holds every required tag.
    pub fn is_satisfied_by(&self, user: &User) -> bool {
        match self {
            WorkerSlot::Specific { user_id, .. } => user.id == *user_id,
            WorkerSlot::Placeholder { required_tags, .. } => {
                required_tags.is_subset(&user.tags)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub dependencies: Vec<Dependency>,
    /// Worker slots for this task — each is either a named person or an open
    /// role placeholder. Workload effort is stored per slot.
    pub workers: Vec<WorkerSlot>,
    /// Optional scheduling constraint.
    pub constraint: Option<DateConstraint>,
    /// Calendar span in working days. 0.0 means derive from workload.
    pub duration_days_target: f32,
}

impl Task {
    /// Create a task with no workers assigned yet.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::NotStarted,
            dependencies: Vec::new(),
            workers: Vec::new(),
            constraint: None,
            duration_days_target: 0.0,
        }
    }

    /// Create a task with `total_days` split equally across the given specific users.
    pub fn with_equal_split(
        name: impl Into<String>,
        description: impl Into<String>,
        total_days: f32,
        users: &[UserId],
    ) -> Self {
        let per_user = if users.is_empty() {
            0.0
        } else {
            total_days / users.len() as f32
        };
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::NotStarted,
            dependencies: Vec::new(),
            workers: users
                .iter()
                .map(|&user_id| WorkerSlot::Specific { user_id, workload_days: per_user })
                .collect(),
            constraint: None,
            duration_days_target: 0.0,
        }
    }

    /// Assign a specific user to this task with the given workload in days.
    pub fn add_specific_worker(&mut self, user_id: UserId, workload_days: f32) {
        self.workers
            .push(WorkerSlot::Specific { user_id, workload_days });
    }

    /// Add an open-role placeholder that any user with the given tags can fill.
    pub fn add_placeholder_worker(
        &mut self,
        required_tags: impl IntoIterator<Item = impl Into<String>>,
        workload_days: f32,
    ) {
        self.workers.push(WorkerSlot::Placeholder {
            required_tags: required_tags.into_iter().map(Into::into).collect(),
            workload_days,
        });
    }

    /// Total workload across all slots.
    pub fn total_workload_days(&self) -> f32 {
        self.workers.iter().map(WorkerSlot::workload_days).sum()
    }

    /// Yields the `UserId` for every `Specific` slot.
    pub fn assigned_users(&self) -> impl Iterator<Item = UserId> + '_ {
        self.workers.iter().filter_map(|slot| match slot {
            WorkerSlot::Specific { user_id, .. } => Some(*user_id),
            WorkerSlot::Placeholder { .. } => None,
        })
    }

    /// Set the calendar duration in days. Negative values are clamped to 0.
    pub fn with_duration(mut self, days: f32) -> Self {
        self.duration_days_target = days.max(0.0);
        self
    }

    /// Effective calendar duration: the explicit `duration_days_target` if set (> 0),
    /// otherwise twice the workload of the heaviest-loaded worker slot.
    /// Returns 0.0 if no workers are assigned and duration is unset.
    pub fn effective_duration_days(&self) -> f32 {
        if self.duration_days_target > 0.0 {
            return self.duration_days_target;
        }
        let max = self
            .workers
            .iter()
            .map(WorkerSlot::workload_days)
            .fold(0.0_f32, f32::max);
        max * 2.0
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
        assert!(t.workers.is_empty());
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
        for slot in &t.workers {
            assert!((slot.workload_days() - 3.0).abs() < f32::EPSILON);
        }
        assert!((t.total_workload_days() - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn equal_split_with_no_users_is_zero() {
        let t = Task::with_equal_split("Build", "", 10.0, &[]);
        assert!(t.workers.is_empty());
        assert_eq!(t.total_workload_days(), 0.0);
    }

    #[test]
    fn assigned_users_returns_only_specific_slots() {
        let u1 = UserId::new();
        let u2 = UserId::new();
        let mut t = Task::new("X", "");
        t.add_specific_worker(u1, 2.0);
        t.add_specific_worker(u2, 3.0);
        t.add_placeholder_worker(["rust"], 1.0);
        let assigned: Vec<_> = t.assigned_users().collect();
        assert_eq!(assigned.len(), 2);
        assert!(assigned.contains(&u1));
        assert!(assigned.contains(&u2));
    }

    #[test]
    fn total_workload_sums_all_slots() {
        let u = UserId::new();
        let mut t = Task::new("X", "");
        t.add_specific_worker(u, 3.0);
        t.add_placeholder_worker(["rust"], 5.0);
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

    // ── WorkerSlot ────────────────────────────────────────────────────────────

    #[test]
    fn specific_slot_satisfied_only_by_pinned_user() {
        let u1 = UserId::new();
        let u2 = UserId::new();
        let slot = WorkerSlot::Specific { user_id: u1, workload_days: 2.0 };
        let alice = User { id: u1, name: "Alice".into(), tags: Default::default() };
        let bob = User { id: u2, name: "Bob".into(), tags: Default::default() };
        assert!(slot.is_satisfied_by(&alice));
        assert!(!slot.is_satisfied_by(&bob));
    }

    #[test]
    fn placeholder_satisfied_by_user_with_all_tags() {
        let slot = WorkerSlot::Placeholder {
            required_tags: ["rust", "skia"].iter().map(|s| s.to_string()).collect(),
            workload_days: 3.0,
        };
        let eligible = User::new("Alice").with_tag("rust").with_tag("skia").with_tag("extra");
        let missing_one = User::new("Bob").with_tag("rust");
        let no_tags = User::new("Carol");
        assert!(slot.is_satisfied_by(&eligible));
        assert!(!slot.is_satisfied_by(&missing_one));
        assert!(!slot.is_satisfied_by(&no_tags));
    }

    #[test]
    fn placeholder_with_no_tags_accepts_any_user() {
        let slot =
            WorkerSlot::Placeholder { required_tags: HashSet::new(), workload_days: 1.0 };
        let u = User::new("Anyone");
        assert!(slot.is_satisfied_by(&u));
    }

    #[test]
    fn add_placeholder_worker_builder() {
        let mut t = Task::new("T", "");
        t.add_placeholder_worker(["rust", "skia"], 4.0);
        assert_eq!(t.workers.len(), 1);
        assert!((t.total_workload_days() - 4.0).abs() < f32::EPSILON);
    }

    // ── duration_days ─────────────────────────────────────────────────────────

    #[test]
    fn default_duration_days_is_zero() {
        let t = Task::new("T", "");
        assert_eq!(t.duration_days_target, 0.0);
    }

    #[test]
    fn with_duration_stores_value() {
        let t = Task::new("T", "").with_duration(5.0);
        assert_eq!(t.duration_days_target, 5.0);
    }

    #[test]
    fn with_duration_clamps_negative_to_zero() {
        let t = Task::new("T", "").with_duration(-1.0);
        assert_eq!(t.duration_days_target, 0.0);
    }

    #[test]
    fn effective_duration_returns_explicit_when_set() {
        let u = UserId::new();
        let t = Task::with_equal_split("T", "", 2.0, &[u]).with_duration(7.0);
        assert_eq!(t.effective_duration_days(), 7.0);
    }

    #[test]
    fn effective_duration_no_workers_no_duration_is_zero() {
        let t = Task::new("T", "");
        assert_eq!(t.effective_duration_days(), 0.0);
    }

    #[test]
    fn effective_duration_one_worker_two_days_workload() {
        let u = UserId::new();
        let t = Task::with_equal_split("T", "", 2.0, &[u]);
        assert!((t.effective_duration_days() - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_duration_two_workers_two_days_each() {
        // max slot = 2.0 → effective = 4.0
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 4.0, &users);
        assert!((t.effective_duration_days() - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_duration_two_workers_one_day_each() {
        // max slot = 1.0 → effective = 2.0
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 2.0, &users);
        assert!((t.effective_duration_days() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_duration_uses_heaviest_slot_not_average() {
        // Two workers with unequal load: max = 6.0 → effective = 12.0
        // (average would be 4.0 → 8.0, which is wrong)
        let u1 = UserId::new();
        let u2 = UserId::new();
        let mut t = Task::new("T", "");
        t.add_specific_worker(u1, 2.0);
        t.add_specific_worker(u2, 6.0);
        assert!((t.effective_duration_days() - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn explicit_duration_overrides_formula() {
        let users = vec![UserId::new(), UserId::new()];
        let t = Task::with_equal_split("T", "", 4.0, &users).with_duration(3.0);
        assert_eq!(t.effective_duration_days(), 3.0);
    }
}
