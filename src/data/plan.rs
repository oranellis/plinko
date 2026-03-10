//! The [`Plan`] aggregate root and its [`DependencyError`] type.

use crate::data::allocation::PlanAllocation;
use crate::data::dependency::Dependency;
use crate::data::ids::NodeId;
use crate::data::{
    CalendarOverrides, Milestone, MilestoneId, StartDates, Task, TaskId, TaskStatus, User, UserId,
    WorkSchedule, WorkerSlot,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub enum DependencyError {
    /// Adding this dependency would create a cycle.
    Cycle,
    /// The source task or milestone does not exist in this plan.
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub name: String,
    pub tasks: HashMap<TaskId, Task>,
    pub milestones: HashMap<MilestoneId, Milestone>,
    pub users: HashMap<UserId, User>,
    /// Default schedule applied to all users unless overridden.
    pub default_schedule: WorkSchedule,
    /// Per-user schedule overrides. Falls back to `default_schedule` when absent.
    pub user_schedules: HashMap<UserId, WorkSchedule>,
    /// Plan-wide calendar exceptions (e.g. bank holidays, company events).
    /// Applies to all users unless a user has their own override for that date.
    pub calendar: CalendarOverrides,
    /// Per-user calendar exceptions. Takes priority over `calendar`.
    pub user_calendars: HashMap<UserId, CalendarOverrides>,
    /// The date the plan begins. Can be used as a dependency via `DependencyId::PlanStart`.
    pub start_date: NaiveDate,
    /// Start dates for all tasks and milestones. Kept separate so they can be
    /// recomputed without touching task definitions.
    pub dates: StartDates,
    /// The node that the scheduler is trying to optimise for (bring as early as possible).
    /// If set to the plan start then all end nodes are brought in as much as possible
    pub scheduler_target: NodeId,
    /// How many standard hours one workload-day represents.
    /// Used to convert `WorkerSlot::workload_days` → hours when filling capacity.
    /// Defaults to 8.0. Kept at plan level so effort is consistent across users
    /// regardless of individual schedule lengths.
    #[serde(default = "Plan::default_hours_per_workload_day")]
    pub hours_per_workload_day: f32,
    /// The latest computed allocation. `None` until the scheduler has been run,
    /// and invalidated (reset to `None`) whenever the plan is mutated.
    #[serde(default)]
    pub allocation: Option<PlanAllocation>,
    /// Ordered registry of all tags (skills/roles) used in this plan.
    /// Tags on users and task placeholders must come from this list.
    /// The order here controls how tags appear in the tag management UI.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Plan {
    fn default_hours_per_workload_day() -> f32 {
        8.0
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            users: HashMap::new(),
            default_schedule: WorkSchedule::default(),
            user_schedules: HashMap::new(),
            calendar: CalendarOverrides::new(),
            user_calendars: HashMap::new(),
            start_date: chrono::Local::now().date_naive(),
            dates: StartDates::new(),
            scheduler_target: NodeId::PlanStart,
            hours_per_workload_day: 8.0,
            allocation: None,
            tags: Vec::new(),
        }
    }

    /// Returns the effective schedule for a user, falling back to the plan default.
    pub fn schedule_for(&self, user_id: &UserId) -> &WorkSchedule {
        self.user_schedules
            .get(user_id)
            .unwrap_or(&self.default_schedule)
    }

    /// Override the schedule for a specific user.
    pub fn set_user_schedule(&mut self, user_id: UserId, schedule: WorkSchedule) {
        self.user_schedules.insert(user_id, schedule);
        self.allocation = None;
    }

    /// Remove a user's schedule override, reverting them to the plan default.
    pub fn clear_user_schedule(&mut self, user_id: &UserId) {
        self.user_schedules.remove(user_id);
        self.allocation = None;
    }

    /// Effective hours available for a user on a specific date.
    /// Resolution order: user calendar → plan calendar → user schedule → plan schedule.
    pub fn hours_available(&self, user_id: &UserId, date: NaiveDate) -> f32 {
        // 1. User-specific date override
        if let Some(h) = self.user_calendars.get(user_id).and_then(|c| c.get(date)) {
            return h;
        }
        // 2. Plan-wide date override
        if let Some(h) = self.calendar.get(date) {
            return h;
        }
        // 3. Normal schedule for that weekday
        let weekday = crate::data::schedule::chrono_to_weekday(date.weekday());
        self.schedule_for(user_id).hours_on(weekday)
    }

    /// Returns all users that possess ALL of the specified tags (skills/roles/clearances).
    /// An empty tag list matches all users.
    pub fn users_with_tags(&self, tags: &[&str]) -> impl Iterator<Item = &User> {
        let tag_set: std::collections::HashSet<&str> = tags.iter().copied().collect();
        self.users
            .values()
            .filter(move |user| tag_set.iter().all(|tag| user.has_tag(tag)))
    }

    /// Add a dependency to a task. Returns `Err` if the task doesn't exist or the
    /// dependency would create a cycle. If a dependency on the same target already
    /// exists, its lag is updated in place.
    pub fn add_task_dependency(
        &mut self,
        task_id: TaskId,
        dep: Dependency,
    ) -> Result<(), DependencyError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(DependencyError::NotFound);
        }
        if self.has_path(dep.id, NodeId::Task(task_id)) {
            return Err(DependencyError::Cycle);
        }
        let task = self.tasks.get_mut(&task_id).unwrap();
        if let Some(existing) = task.dependencies.iter_mut().find(|d| d.id == dep.id) {
            existing.lag_days = dep.lag_days;
        } else {
            task.dependencies.push(dep);
        }
        self.allocation = None;
        Ok(())
    }

    /// Add a dependency to a milestone. Returns `Err` if the milestone doesn't exist or
    /// the dependency would create a cycle. If a dependency on the same target already
    /// exists, its lag is updated in place.
    pub fn add_milestone_dependency(
        &mut self,
        milestone_id: MilestoneId,
        dep: Dependency,
    ) -> Result<(), DependencyError> {
        if !self.milestones.contains_key(&milestone_id) {
            return Err(DependencyError::NotFound);
        }
        if self.has_path(dep.id, NodeId::Milestone(milestone_id)) {
            return Err(DependencyError::Cycle);
        }
        let milestone = self.milestones.get_mut(&milestone_id).unwrap();
        if let Some(existing) = milestone.dependencies.iter_mut().find(|d| d.id == dep.id) {
            existing.lag_days = dep.lag_days;
        } else {
            milestone.dependencies.push(dep);
        }
        self.allocation = None;
        Ok(())
    }

    /// Returns true if `target` is reachable from `start` by following existing dependency edges.
    fn has_path(&self, start: NodeId, target: NodeId) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            match current {
                NodeId::Task(id) => {
                    if let Some(task) = self.tasks.get(&id) {
                        stack.extend(task.dependencies.iter().map(|d| d.id));
                    }
                }
                NodeId::Milestone(id) => {
                    if let Some(milestone) = self.milestones.get(&id) {
                        stack.extend(milestone.dependencies.iter().map(|d| d.id));
                    }
                }
                // PlanStart is the root — it has no dependencies of its own.
                NodeId::PlanStart => {}
            }
        }
        false
    }

    /// Resolve the start date of any dependency. Returns `None` if the task or
    /// milestone has no date assigned yet.
    pub fn start_of(&self, dep: NodeId) -> Option<NaiveDate> {
        match dep {
            NodeId::PlanStart => Some(self.start_date),
            NodeId::Task(id) => self.dates.task(&id),
            NodeId::Milestone(id) => self.dates.milestone(&id),
        }
    }

    /// Mark a task as started: sets its status to `InProgress` and records
    /// today as its start date in `plan.dates`. Invalidates the allocation.
    /// Does nothing if the task does not exist.
    pub fn start_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let today = chrono::Local::now().date_naive();
        self.tasks.get_mut(&id).unwrap().status = TaskStatus::InProgress;
        self.dates.set_task(id, today);
        self.allocation = None;
    }

    /// Mark a task as complete: sets its status to `Complete` and records
    /// today as its `actual_end_date` (used by the scheduler as the anchor
    /// end date for dependent tasks). The start date in `plan.dates` is
    /// preserved. Invalidates the allocation.
    /// Does nothing if the task does not exist.
    pub fn complete_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let today = chrono::Local::now().date_naive();
        let task = self.tasks.get_mut(&id).unwrap();
        task.status = TaskStatus::Complete;
        task.actual_end_date = Some(today);
        self.allocation = None;
    }

    pub fn add_task(&mut self, task: Task) -> TaskId {
        let id = task.id;
        self.tasks.insert(id, task);
        self.allocation = None;
        id
    }

    pub fn add_milestone(&mut self, milestone: Milestone) -> MilestoneId {
        let id = milestone.id;
        self.milestones.insert(id, milestone);
        self.allocation = None;
        id
    }

    pub fn add_user(&mut self, user: User) -> UserId {
        let id = user.id;
        self.users.insert(id, user);
        self.allocation = None;
        id
    }

    // ── Tag registry ──────────────────────────────────────────────────────────

    /// Append `name` to the ordered tag registry.
    /// Does nothing and returns `false` if the tag already exists.
    pub fn add_tag(&mut self, name: impl Into<String>) -> bool {
        let name = name.into();
        if self.tags.iter().any(|t| t == &name) {
            return false;
        }
        self.tags.push(name);
        true
    }

    /// Rename a tag everywhere: the registry, every user's tag set, and every
    /// task placeholder's `required_tags`. Returns `false` if `old` is not in
    /// the registry or `new_name` already exists.
    pub fn rename_tag(&mut self, old: &str, new_name: &str) -> bool {
        let pos = match self.tags.iter().position(|t| t == old) {
            Some(p) => p,
            None => return false,
        };
        if self.tags.iter().any(|t| t == new_name) {
            return false;
        }
        self.tags[pos] = new_name.to_string();
        for user in self.users.values_mut() {
            if user.tags.remove(old) {
                user.tags.insert(new_name.to_string());
            }
        }
        for task in self.tasks.values_mut() {
            for slot in task.workers.iter_mut() {
                if let WorkerSlot::Placeholder { required_tags, .. } = slot
                    && required_tags.remove(old)
                {
                    required_tags.insert(new_name.to_string());
                }
            }
        }
        self.allocation = None;
        true
    }

    /// Remove a tag from the registry, all user tag sets, and all task
    /// placeholder `required_tags`. No-op if the tag is not in the registry.
    pub fn remove_tag(&mut self, name: &str) {
        self.tags.retain(|t| t != name);
        for user in self.users.values_mut() {
            user.tags.remove(name);
        }
        for task in self.tasks.values_mut() {
            for slot in task.workers.iter_mut() {
                if let WorkerSlot::Placeholder { required_tags, .. } = slot {
                    required_tags.remove(name);
                }
            }
        }
        self.allocation = None;
    }

    /// Move `name` to `new_index` in the registry (0-based, clamped to valid
    /// range). Returns `false` if the tag is not in the registry.
    pub fn move_tag(&mut self, name: &str, new_index: usize) -> bool {
        let pos = match self.tags.iter().position(|t| t == name) {
            Some(p) => p,
            None => return false,
        };
        let tag = self.tags.remove(pos);
        let insert_at = new_index.min(self.tags.len());
        self.tags.insert(insert_at, tag);
        true
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Dependency, Milestone, Task, User, Weekday, WorkSchedule};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // 2026-03-09 = Monday, 2026-03-14 = Saturday
    const MON: (i32, u32, u32) = (2026, 3, 9);
    const SAT: (i32, u32, u32) = (2026, 3, 14);

    fn make_plan() -> Plan {
        let mut p = Plan::new("Test");
        p.start_date = date(2026, 1, 1);
        p
    }

    // ── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn new_plan_has_empty_collections() {
        let p = Plan::new("My Plan");
        assert_eq!(p.name, "My Plan");
        assert!(p.tasks.is_empty());
        assert!(p.milestones.is_empty());
        assert!(p.users.is_empty());
    }

    #[test]
    fn add_task_stores_and_returns_id() {
        let mut p = make_plan();
        let t = Task::new("T", "");
        let expected_id = t.id;
        let returned_id = p.add_task(t);
        assert_eq!(returned_id, expected_id);
        assert!(p.tasks.contains_key(&returned_id));
    }

    #[test]
    fn add_milestone_stores_and_returns_id() {
        let mut p = make_plan();
        let m = Milestone::new("M", "");
        let id = p.add_milestone(m);
        assert!(p.milestones.contains_key(&id));
    }

    #[test]
    fn add_user_stores_and_returns_id() {
        let mut p = make_plan();
        let u = User::new("Alice");
        let id = p.add_user(u);
        assert!(p.users.contains_key(&id));
        assert_eq!(p.users[&id].name, "Alice");
    }

    #[test]
    fn users_with_tags_returns_matching_users() {
        let mut p = make_plan();
        p.add_user(User::new("Alice").with_tag("rust").with_tag("frontend"));
        p.add_user(User::new("Bob").with_tag("rust").with_tag("backend"));
        p.add_user(User::new("Carol").with_tag("python"));

        let rust_users: Vec<_> = p
            .users_with_tags(&["rust"])
            .map(|u| u.name.as_str())
            .collect();
        assert_eq!(rust_users.len(), 2);
        assert!(rust_users.contains(&"Alice"));
        assert!(rust_users.contains(&"Bob"));
    }

    #[test]
    fn users_with_tags_returns_users_with_all_tags() {
        let mut p = make_plan();
        p.add_user(User::new("Alice").with_tag("rust").with_tag("frontend"));
        p.add_user(User::new("Bob").with_tag("rust").with_tag("backend"));
        p.add_user(User::new("Carol").with_tag("python"));

        let users: Vec<_> = p
            .users_with_tags(&["rust", "frontend"])
            .map(|u| u.name.as_str())
            .collect();
        assert_eq!(users.len(), 1);
        assert!(users.contains(&"Alice"));
    }

    #[test]
    fn users_with_tags_returns_empty_when_no_matches() {
        let mut p = make_plan();
        p.add_user(User::new("Alice").with_tag("rust"));

        let users: Vec<_> = p.users_with_tags(&["python"]).collect();
        assert!(users.is_empty());
    }

    #[test]
    fn users_with_tags_returns_empty_for_empty_plan() {
        let p = make_plan();
        let users: Vec<_> = p.users_with_tags(&["rust"]).collect();
        assert!(users.is_empty());
    }

    #[test]
    fn users_with_tags_empty_list_matches_all_users() {
        let mut p = make_plan();
        p.add_user(User::new("Alice").with_tag("rust"));
        p.add_user(User::new("Bob").with_tag("python"));
        p.add_user(User::new("Carol"));

        let users: Vec<_> = p.users_with_tags(&[]).collect();
        assert_eq!(users.len(), 3);
    }

    // ── Schedule resolution ───────────────────────────────────────────────────

    #[test]
    fn schedule_for_returns_default_when_no_override() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        assert_eq!(
            p.schedule_for(&uid).total_hours_per_week(),
            p.default_schedule.total_hours_per_week()
        );
    }

    #[test]
    fn schedule_for_returns_user_override() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.set_user_schedule(uid, WorkSchedule::full_week());
        assert_eq!(p.schedule_for(&uid).total_hours_per_week(), 56.0);
    }

    #[test]
    fn clear_user_schedule_reverts_to_default() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.set_user_schedule(uid, WorkSchedule::full_week());
        p.clear_user_schedule(&uid);
        assert_eq!(
            p.schedule_for(&uid).total_hours_per_week(),
            p.default_schedule.total_hours_per_week()
        );
    }

    // ── hours_available ───────────────────────────────────────────────────────

    #[test]
    fn hours_available_weekday_uses_default_schedule() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        assert_eq!(p.hours_available(&uid, date(MON.0, MON.1, MON.2)), 8.0);
    }

    #[test]
    fn hours_available_weekend_is_zero_by_default() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        assert_eq!(p.hours_available(&uid, date(SAT.0, SAT.1, SAT.2)), 0.0);
    }

    #[test]
    fn hours_available_plan_calendar_overrides_schedule() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.calendar.set(date(MON.0, MON.1, MON.2), 3.0);
        assert_eq!(p.hours_available(&uid, date(MON.0, MON.1, MON.2)), 3.0);
    }

    #[test]
    fn hours_available_user_calendar_takes_priority_over_plan_calendar() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.calendar.set(date(MON.0, MON.1, MON.2), 3.0);
        p.user_calendars
            .entry(uid)
            .or_default()
            .set(date(MON.0, MON.1, MON.2), 1.0);
        assert_eq!(p.hours_available(&uid, date(MON.0, MON.1, MON.2)), 1.0);
    }

    #[test]
    fn hours_available_user_calendar_does_not_affect_other_users() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));
        let bob = p.add_user(User::new("Bob"));
        p.user_calendars
            .entry(alice)
            .or_default()
            .set(date(MON.0, MON.1, MON.2), 2.0);
        assert_eq!(p.hours_available(&bob, date(MON.0, MON.1, MON.2)), 8.0);
    }

    #[test]
    fn hours_available_user_schedule_override_applies_to_weekday() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.set_user_schedule(uid, WorkSchedule::weekdays().with_day(Weekday::Monday, 4.0));
        assert_eq!(p.hours_available(&uid, date(MON.0, MON.1, MON.2)), 4.0);
    }

    #[test]
    fn hours_available_plan_calendar_zero_means_day_off() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        p.calendar.set(date(MON.0, MON.1, MON.2), 0.0);
        assert_eq!(p.hours_available(&uid, date(MON.0, MON.1, MON.2)), 0.0);
    }

    // ── Dependency management ─────────────────────────────────────────────────

    fn has_dep(deps: &[Dependency], id: NodeId) -> bool {
        deps.iter().any(|d| d.id == id)
    }

    #[test]
    fn add_task_dependency_happy_path() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        assert!(
            p.add_task_dependency(b, Dependency::new(NodeId::Task(a)))
                .is_ok()
        );
        assert!(has_dep(&p.tasks[&b].dependencies, NodeId::Task(a)));
    }

    #[test]
    fn add_task_dependency_returns_not_found_for_unknown_task() {
        let mut p = make_plan();
        let unknown = TaskId::new();
        let other = p.add_task(Task::new("Other", ""));
        assert_eq!(
            p.add_task_dependency(unknown, Dependency::new(NodeId::Task(other))),
            Err(DependencyError::NotFound)
        );
    }

    #[test]
    fn add_task_dependency_detects_direct_cycle() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        p.add_task_dependency(a, Dependency::new(NodeId::Task(b)))
            .unwrap();
        assert_eq!(
            p.add_task_dependency(b, Dependency::new(NodeId::Task(a))),
            Err(DependencyError::Cycle)
        );
    }

    #[test]
    fn add_task_dependency_detects_indirect_cycle() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        let c = p.add_task(Task::new("C", ""));
        p.add_task_dependency(a, Dependency::new(NodeId::Task(b)))
            .unwrap();
        p.add_task_dependency(b, Dependency::new(NodeId::Task(c)))
            .unwrap();
        assert_eq!(
            p.add_task_dependency(c, Dependency::new(NodeId::Task(a))),
            Err(DependencyError::Cycle)
        );
    }

    #[test]
    fn add_task_dependency_same_id_updates_lag_not_duplicate() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        p.add_task_dependency(b, Dependency::new(NodeId::Task(a)))
            .unwrap();
        p.add_task_dependency(b, Dependency::with_lag(NodeId::Task(a), 3.0))
            .unwrap();
        assert_eq!(p.tasks[&b].dependencies.len(), 1);
        assert_eq!(p.tasks[&b].dependencies[0].lag_days, 3.0);
    }

    #[test]
    fn add_task_dependency_with_lag() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        p.add_task_dependency(b, Dependency::with_lag(NodeId::Task(a), 5.0))
            .unwrap();
        assert_eq!(p.tasks[&b].dependencies[0].lag_days, 5.0);
    }

    #[test]
    fn add_task_dependency_with_lead() {
        let mut p = make_plan();
        let a = p.add_task(Task::new("A", ""));
        let b = p.add_task(Task::new("B", ""));
        p.add_task_dependency(b, Dependency::with_lead(NodeId::Task(a), 2.0))
            .unwrap();
        assert_eq!(p.tasks[&b].dependencies[0].lag_days, -2.0);
    }

    #[test]
    fn add_milestone_dependency_happy_path() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        let m = p.add_milestone(Milestone::new("M", ""));
        assert!(
            p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t)))
                .is_ok()
        );
        assert!(has_dep(&p.milestones[&m].dependencies, NodeId::Task(t)));
    }

    #[test]
    fn add_milestone_dependency_returns_not_found_for_unknown_milestone() {
        let mut p = make_plan();
        let unknown = MilestoneId::new();
        assert_eq!(
            p.add_milestone_dependency(unknown, Dependency::new(NodeId::PlanStart)),
            Err(DependencyError::NotFound)
        );
    }

    #[test]
    fn add_milestone_dependency_detects_direct_cycle() {
        let mut p = make_plan();
        let m1 = p.add_milestone(Milestone::new("M1", ""));
        let m2 = p.add_milestone(Milestone::new("M2", ""));
        p.add_milestone_dependency(m1, Dependency::new(NodeId::Milestone(m2)))
            .unwrap();
        assert_eq!(
            p.add_milestone_dependency(m2, Dependency::new(NodeId::Milestone(m1))),
            Err(DependencyError::Cycle)
        );
    }

    #[test]
    fn add_milestone_dependency_same_id_updates_lag() {
        let mut p = make_plan();
        let m1 = p.add_milestone(Milestone::new("M1", ""));
        let m2 = p.add_milestone(Milestone::new("M2", ""));
        p.add_milestone_dependency(m1, Dependency::new(NodeId::Milestone(m2)))
            .unwrap();
        p.add_milestone_dependency(m1, Dependency::with_lag(NodeId::Milestone(m2), 1.0))
            .unwrap();
        assert_eq!(p.milestones[&m1].dependencies.len(), 1);
        assert_eq!(p.milestones[&m1].dependencies[0].lag_days, 1.0);
    }

    #[test]
    fn cross_type_cycle_task_to_milestone_is_detected() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        let m = p.add_milestone(Milestone::new("M", ""));
        p.add_task_dependency(t, Dependency::new(NodeId::Milestone(m)))
            .unwrap();
        assert_eq!(
            p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t))),
            Err(DependencyError::Cycle)
        );
    }

    #[test]
    fn plan_start_can_be_added_as_task_dependency() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        assert!(
            p.add_task_dependency(t, Dependency::new(NodeId::PlanStart))
                .is_ok()
        );
    }

    #[test]
    fn plan_start_cannot_create_a_cycle() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        p.add_task_dependency(t, Dependency::new(NodeId::PlanStart))
            .unwrap();
        let t2 = p.add_task(Task::new("T2", ""));
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t)))
            .unwrap();
        assert!(
            p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
                .is_ok()
        );
    }

    // ── start_of ──────────────────────────────────────────────────────────────

    #[test]
    fn start_of_plan_start_returns_plan_start_date() {
        let p = make_plan();
        assert_eq!(p.start_of(NodeId::PlanStart), Some(date(2026, 1, 1)));
    }

    #[test]
    fn start_of_task_returns_assigned_date() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        p.dates.set_task(t, date(2026, 3, 9));
        assert_eq!(p.start_of(NodeId::Task(t)), Some(date(2026, 3, 9)));
    }

    #[test]
    fn start_of_unscheduled_task_is_none() {
        let mut p = make_plan();
        let t = p.add_task(Task::new("T", ""));
        assert_eq!(p.start_of(NodeId::Task(t)), None);
    }

    #[test]
    fn start_of_milestone_returns_assigned_date() {
        let mut p = make_plan();
        let m = p.add_milestone(Milestone::new("M", ""));
        p.dates.set_milestone(m, date(2026, 6, 1));
        assert_eq!(p.start_of(NodeId::Milestone(m)), Some(date(2026, 6, 1)));
    }

    #[test]
    fn start_of_unscheduled_milestone_is_none() {
        let mut p = make_plan();
        let m = p.add_milestone(Milestone::new("M", ""));
        assert_eq!(p.start_of(NodeId::Milestone(m)), None);
    }

    // ── start_task / complete_task ────────────────────────────────────────────

    #[test]
    fn start_task_sets_status_and_date() {
        let today = chrono::Local::now().date_naive();
        let mut p = make_plan();
        let tid = p.add_task(Task::new("T", ""));

        p.start_task(tid);

        assert_eq!(p.tasks[&tid].status, TaskStatus::InProgress);
        assert_eq!(p.dates.task(&tid), Some(today));
    }

    #[test]
    fn start_task_clears_allocation() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));
        let mut t = Task::new("T", "");
        t.add_specific_worker(alice, 1.0);
        let tid = p.add_task(t);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.compute_time_optimised_plan().unwrap();
        assert!(p.allocation.is_some());

        p.start_task(tid);
        assert!(p.allocation.is_none());
    }

    #[test]
    fn start_task_unknown_id_is_noop() {
        let mut p = make_plan();
        let unknown = TaskId::new();
        p.start_task(unknown); // must not panic
    }

    #[test]
    fn start_task_overwrites_existing_date() {
        let today = chrono::Local::now().date_naive();
        let mut p = make_plan();
        let tid = p.add_task(Task::new("T", ""));
        p.dates.set_task(tid, date(2025, 1, 1));

        p.start_task(tid);

        assert_eq!(p.dates.task(&tid), Some(today));
    }

    #[test]
    fn complete_task_sets_status_and_actual_end_date() {
        let today = chrono::Local::now().date_naive();
        let mut p = make_plan();
        let tid = p.add_task(Task::new("T", ""));

        p.complete_task(tid);

        assert_eq!(p.tasks[&tid].status, TaskStatus::Complete);
        assert_eq!(p.tasks[&tid].actual_end_date, Some(today));
    }

    #[test]
    fn complete_task_preserves_existing_start_date() {
        let start = date(2025, 6, 1);
        let mut p = make_plan();
        let tid = p.add_task(Task::new("T", ""));
        p.dates.set_task(tid, start);

        p.complete_task(tid);

        assert_eq!(
            p.dates.task(&tid),
            Some(start),
            "start date must be unchanged"
        );
    }

    #[test]
    fn complete_task_clears_allocation() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));
        let mut t = Task::new("T", "");
        t.add_specific_worker(alice, 1.0);
        let tid = p.add_task(t);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.compute_time_optimised_plan().unwrap();
        assert!(p.allocation.is_some());

        p.complete_task(tid);
        assert!(p.allocation.is_none());
    }

    #[test]
    fn complete_task_unknown_id_is_noop() {
        let mut p = make_plan();
        p.complete_task(TaskId::new()); // must not panic
    }

    // ── Tag registry ──────────────────────────────────────────────────────────

    #[test]
    fn add_tag_appends_and_returns_true() {
        let mut p = make_plan();
        assert!(p.add_tag("rust"));
        assert!(p.add_tag("python"));
        assert_eq!(p.tags, vec!["rust", "python"]);
    }

    #[test]
    fn add_tag_ignores_duplicate() {
        let mut p = make_plan();
        p.add_tag("rust");
        assert!(!p.add_tag("rust"));
        assert_eq!(p.tags.len(), 1);
    }

    #[test]
    fn rename_tag_updates_registry_users_and_tasks() {
        let mut p = make_plan();
        p.add_tag("rust");
        let uid = p.add_user(User::new("Alice").with_tag("rust"));
        let mut t = Task::new("T", "");
        t.workers.push(WorkerSlot::Placeholder {
            required_tags: ["rust".to_string()].into(),
            workload_days: 1.0,
        });
        let tid = p.add_task(t);

        assert!(p.rename_tag("rust", "typescript"));
        assert_eq!(p.tags, vec!["typescript"]);
        assert!(p.users[&uid].tags.contains("typescript"));
        assert!(!p.users[&uid].tags.contains("rust"));
        if let WorkerSlot::Placeholder { required_tags, .. } = &p.tasks[&tid].workers[0] {
            assert!(required_tags.contains("typescript"));
            assert!(!required_tags.contains("rust"));
        } else {
            panic!("expected placeholder");
        }
    }

    #[test]
    fn rename_tag_fails_if_old_not_found() {
        let mut p = make_plan();
        assert!(!p.rename_tag("missing", "new"));
    }

    #[test]
    fn rename_tag_fails_if_new_name_already_exists() {
        let mut p = make_plan();
        p.add_tag("rust");
        p.add_tag("python");
        assert!(!p.rename_tag("rust", "python"));
        assert_eq!(p.tags, vec!["rust", "python"]);
    }

    #[test]
    fn remove_tag_removes_from_registry_users_and_tasks() {
        let mut p = make_plan();
        p.add_tag("rust");
        p.add_tag("python");
        let uid = p.add_user(User::new("Alice").with_tag("rust").with_tag("python"));
        let mut t = Task::new("T", "");
        t.workers.push(WorkerSlot::Placeholder {
            required_tags: ["rust".to_string(), "python".to_string()].into(),
            workload_days: 1.0,
        });
        let tid = p.add_task(t);

        p.remove_tag("rust");
        assert_eq!(p.tags, vec!["python"]);
        assert!(!p.users[&uid].tags.contains("rust"));
        assert!(p.users[&uid].tags.contains("python"));
        if let WorkerSlot::Placeholder { required_tags, .. } = &p.tasks[&tid].workers[0] {
            assert!(!required_tags.contains("rust"));
            assert!(required_tags.contains("python"));
        }
    }

    #[test]
    fn remove_tag_noop_if_not_in_registry() {
        let mut p = make_plan();
        p.remove_tag("missing"); // must not panic
    }

    #[test]
    fn move_tag_reorders_registry() {
        let mut p = make_plan();
        p.add_tag("a");
        p.add_tag("b");
        p.add_tag("c");
        assert!(p.move_tag("a", 2));
        assert_eq!(p.tags, vec!["b", "c", "a"]);
    }

    #[test]
    fn move_tag_clamps_to_end() {
        let mut p = make_plan();
        p.add_tag("a");
        p.add_tag("b");
        assert!(p.move_tag("a", 100));
        assert_eq!(p.tags, vec!["b", "a"]);
    }

    #[test]
    fn move_tag_returns_false_if_not_found() {
        let mut p = make_plan();
        assert!(!p.move_tag("missing", 0));
    }

    #[test]
    fn tags_default_empty_on_old_plan_json() {
        // Serialize a fresh plan (no tags field), then strip the tags key and
        // deserialise to confirm the #[serde(default)] kicks in.
        let mut p = make_plan();
        let json = serde_json::to_string(&p).unwrap();
        // Remove the tags field to simulate an old snapshot.
        let json_no_tags = json.replace(r#","tags":[]"#, "");
        let loaded: Plan = serde_json::from_str(&json_no_tags).expect("deserialize");
        assert!(loaded.tags.is_empty());
        // And a plan with tags should round-trip correctly.
        p.add_tag("rust");
        p.add_tag("python");
        let path = std::env::temp_dir().join(format!("tags_test_{}.json", p.id));
        p.save(&path).unwrap();
        let loaded2 = Plan::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded2.tags, vec!["rust", "python"]);
    }

    // ── Serialization round-trip ──────────────────────────────────────────────

    #[test]
    fn save_and_load_round_trip() {
        let mut p = make_plan();
        let uid = p.add_user(User::new("Alice"));
        let tid = p.add_task(Task::new("Build", "Core work"));
        let mid = p.add_milestone(Milestone::new("Launch", ""));
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.calendar.set(date(2026, 3, 9), 3.0);
        p.dates.set_task(tid, date(2026, 3, 9));
        p.set_user_schedule(uid, WorkSchedule::full_week());

        let path = std::env::temp_dir().join(format!("skiatest_plan_{}.json", p.id));
        p.save(&path).expect("save failed");
        let loaded = Plan::load(&path).expect("load failed");
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.name, p.name);
        assert_eq!(loaded.start_date, p.start_date);
        assert!(loaded.tasks.contains_key(&tid));
        assert!(loaded.milestones.contains_key(&mid));
        assert!(loaded.users.contains_key(&uid));
        assert_eq!(loaded.calendar.get(date(2026, 3, 9)), Some(3.0));
        assert_eq!(loaded.dates.task(&tid), Some(date(2026, 3, 9)));
        assert_eq!(loaded.schedule_for(&uid).total_hours_per_week(), 56.0);
        assert!(has_dep(&loaded.tasks[&tid].dependencies, NodeId::PlanStart));
    }
}
