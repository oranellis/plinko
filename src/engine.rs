//! Request-queue engine — the single gateway through which all Plan mutations flow.
//!
//! [`PlanEngine`] owns the live [`Plan`] and processes [`PlanRequest`]s dequeued
//! from an [`mpsc`] channel. The UI (and later, a network client) submits requests
//! via a clonable [`PlanRequestSender`]; the application main loop drains the
//! queue at the end of each event cycle and acts on the returned [`PlanResponse`]s.
//!
//! # Validation
//!
//! Any mutation that could invalidate an existing schedule is *validated*: if a
//! [`PlanAllocation`](crate::data::PlanAllocation) is present when the request
//! arrives, the engine clones the plan, applies the change, and re-runs the
//! scheduler. On failure the backup is restored and [`PlanError::Scheduler`] is
//! returned. Mutations that only add capacity (e.g. `CreateUser`) and task
//! lifecycle transitions are not validated, as they cannot break an existing
//! schedule by themselves.
//!
//! # Lifecycle
//! ```text
//! UI / network  ──send──►  PlanRequestSender
//!                                │
//!                         mpsc channel
//!                                │
//!              app event loop ◄──┘
//!                    │
//!              PlanEngine::process_pending()
//!                    │
//!              Vec<PlanResponse>
//!                    │
//!         PlanUpdated → mark DirtyRegion::All
//!         Error       → surface to UI
//! ```

use std::collections::HashSet;
use std::sync::mpsc;

use chrono::NaiveDate;

use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::{MilestoneId, NodeId, TagId, TaskId, UserId};
use crate::data::plan::DependencyError;
use crate::data::scheduler::SchedulerError;
use crate::data::task::WorkerSlot;
use crate::data::{Plan, WorkSchedule};

// ── Patch types ───────────────────────────────────────────────────────────────

/// A partial update to a [`Task`](crate::data::Task).
///
/// Every field is `Option`-wrapped; `None` means "leave unchanged".
/// Nullable plan fields use `Option<Option<T>>`:
/// `Some(None)` clears the field, `Some(Some(v))` sets it, `None` leaves it.
///
/// Build with the chainable setters:
/// ```ignore
/// let patch = TaskPatch::new().name("Revised title").duration_days_target(3.0);
/// ```
#[derive(Default)]
pub struct TaskPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Directly overrides the task status. Bypasses lifecycle date-recording;
    /// use alongside `actual_start_date` / `actual_end_date` when needed.
    pub status: Option<crate::data::Status>,
    /// `Some(None)` clears the recorded start date.
    pub actual_start_date: Option<Option<NaiveDate>>,
    /// `Some(None)` clears the recorded end date.
    pub actual_end_date: Option<Option<NaiveDate>>,
    /// `Some(None)` removes the constraint.
    pub constraint: Option<Option<DateConstraint>>,
    /// Replaces the calendar-duration target (0 = derive from workload).
    pub duration_days_target: Option<f32>,
    /// Replaces the full worker-slot list.
    pub workers: Option<Vec<WorkerSlot>>,
    /// Replaces the full dependency list. Cycle detection is run by the engine;
    /// the patch is rejected with [`PlanError::Dependency`] on failure.
    pub dependencies: Option<Vec<Dependency>>,
}

impl TaskPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, v: impl Into<String>) -> Self {
        self.name = Some(v.into());
        self
    }
    pub fn description(mut self, v: impl Into<String>) -> Self {
        self.description = Some(v.into());
        self
    }
    pub fn status(mut self, v: crate::data::Status) -> Self {
        self.status = Some(v);
        self
    }
    pub fn actual_start_date(mut self, v: Option<NaiveDate>) -> Self {
        self.actual_start_date = Some(v);
        self
    }
    pub fn actual_end_date(mut self, v: Option<NaiveDate>) -> Self {
        self.actual_end_date = Some(v);
        self
    }
    pub fn constraint(mut self, v: Option<DateConstraint>) -> Self {
        self.constraint = Some(v);
        self
    }
    pub fn duration_days_target(mut self, v: f32) -> Self {
        self.duration_days_target = Some(v);
        self
    }
    pub fn workers(mut self, v: Vec<WorkerSlot>) -> Self {
        self.workers = Some(v);
        self
    }
    pub fn dependencies(mut self, v: Vec<Dependency>) -> Self {
        self.dependencies = Some(v);
        self
    }
}

/// A partial update to a [`Milestone`](crate::data::Milestone).
///
/// Same `Option`-wrapping conventions as [`TaskPatch`].
#[derive(Default)]
pub struct MilestonePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `Some(None)` removes the constraint.
    pub constraint: Option<Option<DateConstraint>>,
    /// Replaces the full dependency list. Cycle detection is run by the engine.
    pub dependencies: Option<Vec<Dependency>>,
}

impl MilestonePatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, v: impl Into<String>) -> Self {
        self.name = Some(v.into());
        self
    }
    pub fn description(mut self, v: impl Into<String>) -> Self {
        self.description = Some(v.into());
        self
    }
    pub fn constraint(mut self, v: Option<DateConstraint>) -> Self {
        self.constraint = Some(v);
        self
    }
    pub fn dependencies(mut self, v: Vec<Dependency>) -> Self {
        self.dependencies = Some(v);
        self
    }
}

/// A partial update to a [`User`](crate::data::User).
#[derive(Default)]
pub struct UserPatch {
    pub name: Option<String>,
    /// Replaces the user's entire tag set.
    pub tags: Option<HashSet<TagId>>,
    /// `Some(None)` clears the avatar; `Some(Some(v))` sets it to new bytes; `None` leaves it unchanged.
    pub avatar: Option<Option<Vec<u8>>>,
}

impl UserPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, v: impl Into<String>) -> Self {
        self.name = Some(v.into());
        self
    }
    pub fn tags(mut self, v: HashSet<TagId>) -> Self {
        self.tags = Some(v);
        self
    }
    pub fn avatar(mut self, _v: Option<Vec<u8>>) -> Self {
        // avatar field removed from User; kept for API compatibility
        self
    }
}

// ── Requests ──────────────────────────────────────────────────────────────────

/// Every mutation a caller can request of the plan engine.
pub enum PlanRequest {
    // ── Scheduling ────────────────────────────────────────────────────────────
    /// Re-run the time-optimised scheduler and store the result on the plan.
    RunScheduler,

    // ── Task lifecycle (not validated — these are real-world events) ──────────
    /// Transition a task to `InProgress`, recording today as its start date.
    StartTask(TaskId),
    /// Transition a task from `InProgress` to `OnHold`.
    PauseTask(TaskId),
    /// Transition a task from `OnHold` back to `InProgress`.
    ResumeTask(TaskId),
    /// Transition a task to `Complete`, recording today as its end date.
    CompleteTask(TaskId),
    /// Transition a task to `Dropped`.
    DropTask(TaskId),

    // ── Task CRUD (validated if allocation exists) ────────────────────────────
    /// Add a new task. Never validated — adding a disconnected task is always safe.
    CreateTask(crate::data::Task),
    /// Apply a partial update to an existing task. Validated.
    UpdateTask(TaskId, TaskPatch),
    /// Remove a task. Validated — dependent tasks would be broken.
    DeleteTask(TaskId),

    // ── Milestone CRUD (validated if allocation exists) ───────────────────────
    /// Add a new milestone. Never validated.
    CreateMilestone(crate::data::Milestone),
    /// Apply a partial update to an existing milestone. Validated.
    UpdateMilestone(MilestoneId, MilestonePatch),
    /// Remove a milestone. Validated.
    DeleteMilestone(MilestoneId),

    // ── User CRUD (validated if allocation exists) ────────────────────────────
    /// Add a new user. Never validated — adding capacity is always safe.
    CreateUser(crate::data::User),
    /// Apply a partial update to an existing user. Validated — tag changes can
    /// make placeholder slots unsatisfiable.
    UpdateUser(UserId, UserPatch),
    /// Remove a user. Validated — tasks assigned to this user would fail.
    DeleteUser(UserId),
    /// Set or replace a user's per-user schedule override. Validated — capacity
    /// changes may break the existing schedule.
    SetUserSchedule(UserId, WorkSchedule),
    /// Remove a user's schedule override, reverting to the plan default. Validated.
    ClearUserSchedule(UserId),
    /// Set the plan's default work schedule. Validated — capacity changes may
    /// break the existing schedule.
    SetDefaultSchedule(WorkSchedule),
    /// Set a plan-level calendar override for a specific date.
    /// `hours = 0.0` marks the date as a holiday.  Validated — removing
    /// capacity may break an existing schedule.
    SetCalendarOverride(NaiveDate, f32),
    /// Remove a plan-level calendar override, reverting the date to the
    /// normal schedule.  Validated — restoring capacity is safe but the
    /// closure still runs through the validator for consistency.
    ClearCalendarOverride(NaiveDate),
    /// Set a per-user calendar override for a specific date.
    SetUserCalendarOverride(UserId, NaiveDate, f32),
    /// Remove a per-user calendar override for a specific date.
    ClearUserCalendarOverride(UserId, NaiveDate),
    /// Replace the entire plan (used for load / new plan). Runs the scheduler.
    ReplacePlan(Box<Plan>),

    // ── Tag registry ──────────────────────────────────────────────────────────
    /// Append a new tag to the plan's ordered tag registry. No-op if it already
    /// exists.
    AddTag(String),
    /// Rename a tag in the registry by ID.
    RenameTag(TagId, String),
    /// Remove a tag from the registry and strip it from all users and task
    /// placeholders.
    DeleteTag(TagId),
    /// Move a tag to a new position in the registry (controls UI display order).
    MoveTag(TagId, usize),

    // ── Plan metadata ──────────────────────────────────────────────────────────
    /// Update top-level plan metadata.
    UpdatePlanSettings {
        name: String,
        start_date: chrono::NaiveDate,
        scheduler_target: NodeId,
    },
}

// ── Responses ─────────────────────────────────────────────────────────────────

/// The outcome of processing a single [`PlanRequest`].
#[derive(Debug)]
pub enum PlanResponse {
    /// The plan was successfully mutated. The UI should re-read from the engine
    /// and re-render.
    PlanUpdated,
    /// The request could not be applied.
    Error(PlanError),
}

/// Reasons a [`PlanRequest`] can fail.
#[derive(Debug)]
pub enum PlanError {
    /// The referenced task does not exist in the plan.
    TaskNotFound(TaskId),
    /// The referenced milestone does not exist in the plan.
    MilestoneNotFound(MilestoneId),
    /// The referenced user does not exist in the plan.
    UserNotFound(UserId),
    /// The scheduler could not produce a valid schedule.
    Scheduler(SchedulerError),
    /// A dependency operation would create a cycle or reference a missing node.
    Dependency(DependencyError),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::TaskNotFound(id) => write!(f, "task {id:?} not found"),
            PlanError::MilestoneNotFound(id) => write!(f, "milestone {id:?} not found"),
            PlanError::UserNotFound(id) => write!(f, "user {id:?} not found"),
            PlanError::Scheduler(e) => write!(f, "scheduler error: {e}"),
            PlanError::Dependency(DependencyError::Cycle) => {
                write!(f, "dependency would create a cycle")
            }
            PlanError::Dependency(DependencyError::NotFound) => {
                write!(f, "dependency target not found in plan")
            }
        }
    }
}

// ── Sender handle ─────────────────────────────────────────────────────────────

/// A clonable, `Send`-safe handle for submitting [`PlanRequest`]s to the engine.
///
/// Cloning is cheap — each clone shares the same underlying channel. A sender
/// can be given to UI components, page structs, or (later) network threads.
#[derive(Clone)]
pub struct PlanRequestSender(mpsc::Sender<PlanRequest>);

impl PlanRequestSender {
    /// Submit a request. Returns immediately; processing happens the next time
    /// the application main loop calls [`PlanEngine::process_pending`].
    ///
    /// Silently drops the request if the engine has been shut down (receiver
    /// dropped), which only happens during application teardown.
    pub fn send(&self, request: PlanRequest) {
        let _ = self.0.send(request);
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// Owns the live [`Plan`] and processes incoming [`PlanRequest`]s.
pub struct PlanEngine {
    plan: Plan,
    rx: mpsc::Receiver<PlanRequest>,
    tx: mpsc::Sender<PlanRequest>,
}

impl PlanEngine {
    /// Create a new engine wrapping `plan`.
    pub fn new(plan: Plan) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { plan, rx, tx }
    }

    /// Return a clonable sender that can be given to UI components or threads.
    pub fn sender(&self) -> PlanRequestSender {
        PlanRequestSender(self.tx.clone())
    }

    /// Read-only access to the current plan state.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Drain all pending requests, process each one, and return the responses
    /// in order. Call this once per event cycle before rendering.
    ///
    /// After every successful mutation the scheduler is re-run so the Gantt
    /// chart always reflects the latest computed dates.
    pub fn process_pending(&mut self) -> Vec<PlanResponse> {
        let mut responses = Vec::new();
        while let Ok(request) = self.rx.try_recv() {
            let response = self.process(request);
            // If the mutation succeeded but the scheduler wasn't already run
            // inside `process` (e.g. task lifecycle ops), run it now.
            if matches!(response, PlanResponse::PlanUpdated)
                && !self.plan.node_allocations.has_schedule()
            {
                let _ = self.plan.compute_time_optimised_plan();
            }
            if matches!(response, PlanResponse::PlanUpdated) {
                debug_print_plan(&self.plan);
            }
            responses.push(response);
        }
        responses
    }

    // ── Validation helper ─────────────────────────────────────────────────────

    /// Apply a mutation `f` to the plan, then re-run the scheduler.
    ///
    /// If there was an existing allocation and the scheduler fails after the
    /// mutation, the pre-mutation plan is restored and an error is returned.
    /// If there was no existing allocation, the mutation is kept regardless of
    /// scheduler outcome (so newly added tasks are visible on the Gantt).
    fn apply_validated<F>(&mut self, f: F) -> PlanResponse
    where
        F: FnOnce(&mut Plan) -> Result<(), PlanError>,
    {
        // Only back up when there is an existing allocation to protect.
        let backup = self
            .plan
            .node_allocations
            .has_schedule()
            .then(|| self.plan.clone());

        match f(&mut self.plan) {
            Err(e) => PlanResponse::Error(e),
            Ok(()) => match self.plan.compute_time_optimised_plan() {
                Ok(()) => PlanResponse::PlanUpdated,
                Err(e) => {
                    if let Some(backup_plan) = backup {
                        // Had a good schedule before; restore it to keep plan consistent.
                        self.plan = backup_plan;
                        PlanResponse::Error(PlanError::Scheduler(e))
                    } else {
                        // No prior schedule to protect; keep the mutation but log failure.
                        eprintln!("scheduler warning after mutation: {e:?}");
                        PlanResponse::PlanUpdated
                    }
                }
            },
        }
    }

    // ── Internal dispatch ─────────────────────────────────────────────────────

    fn process(&mut self, request: PlanRequest) -> PlanResponse {
        match request {
            // ── Scheduler ─────────────────────────────────────────────────────
            PlanRequest::RunScheduler => match self.plan.compute_time_optimised_plan() {
                Ok(()) => PlanResponse::PlanUpdated,
                Err(e) => PlanResponse::Error(PlanError::Scheduler(e)),
            },

            // ── Task lifecycle (not validated) ────────────────────────────────
            PlanRequest::StartTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.start_task(id);
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }

            PlanRequest::PauseTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.pause_task(id);
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }

            PlanRequest::ResumeTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.resume_task(id);
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }

            PlanRequest::CompleteTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.complete_task(id);
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }

            PlanRequest::DropTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.drop_task(id);
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }

            // ── Task CRUD ─────────────────────────────────────────────────────
            PlanRequest::CreateTask(task) => {
                self.plan.add_task(task);
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdateTask(id, patch) => {
                self.apply_validated(|plan| apply_task_patch(plan, id, patch))
            }

            PlanRequest::DeleteTask(id) => self.apply_validated(|plan| {
                if plan.tasks.remove(&id).is_some() {
                    plan.node_allocations.invalidate();
                    Ok(())
                } else {
                    Err(PlanError::TaskNotFound(id))
                }
            }),

            // ── Milestone CRUD ────────────────────────────────────────────────
            PlanRequest::CreateMilestone(milestone) => {
                self.plan.add_milestone(milestone);
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdateMilestone(id, patch) => {
                self.apply_validated(|plan| apply_milestone_patch(plan, id, patch))
            }

            PlanRequest::DeleteMilestone(id) => self.apply_validated(|plan| {
                if plan.milestones.remove(&id).is_some() {
                    plan.node_allocations.invalidate();
                    Ok(())
                } else {
                    Err(PlanError::MilestoneNotFound(id))
                }
            }),

            // ── User CRUD ─────────────────────────────────────────────────────
            PlanRequest::CreateUser(user) => {
                self.plan.add_user(user);
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdateUser(id, patch) => self.apply_validated(|plan| {
                let user = plan
                    .users_data
                    .get_mut(&id)
                    .map(|ud| &mut ud.user)
                    .ok_or(PlanError::UserNotFound(id))?;
                if let Some(v) = patch.name {
                    user.name = v;
                }
                if let Some(v) = patch.tags {
                    user.tags = v;
                }
                // avatar field not present on User; ignore patch.avatar
                plan.node_allocations.invalidate();
                Ok(())
            }),

            PlanRequest::DeleteUser(id) => self.apply_validated(|plan| {
                if plan.users_data.remove(&id).is_some() {
                    plan.user_calendar_overrides.remove(&id);
                    plan.node_allocations.invalidate();
                    Ok(())
                } else {
                    Err(PlanError::UserNotFound(id))
                }
            }),

            PlanRequest::SetUserSchedule(id, schedule) => self.apply_validated(|plan| {
                if !plan.users_data.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.set_user_schedule(id, schedule);
                Ok(())
            }),

            PlanRequest::ClearUserSchedule(id) => self.apply_validated(|plan| {
                if !plan.users_data.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.clear_user_schedule(&id);
                Ok(())
            }),

            PlanRequest::SetDefaultSchedule(schedule) => self.apply_validated(|plan| {
                plan.default_schedule = schedule;
                Ok(())
            }),

            PlanRequest::SetCalendarOverride(date, hours) => self.apply_validated(|plan| {
                plan.calendar.set(date, hours);
                Ok(())
            }),

            PlanRequest::ClearCalendarOverride(date) => self.apply_validated(|plan| {
                plan.calendar.remove(&date);
                Ok(())
            }),

            PlanRequest::SetUserCalendarOverride(user_id, date, hours) => {
                self.apply_validated(|plan| {
                    plan.user_calendar_overrides
                        .entry(user_id)
                        .or_default()
                        .set(date, hours);
                    Ok(())
                })
            }

            PlanRequest::ClearUserCalendarOverride(user_id, date) => self.apply_validated(|plan| {
                if let Some(cal) = plan.user_calendar_overrides.get_mut(&user_id) {
                    cal.remove(&date);
                }
                Ok(())
            }),

            PlanRequest::ReplacePlan(new_plan) => {
                self.plan = *new_plan;
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }

            // ── Tag registry ──────────────────────────────────────────────────
            PlanRequest::AddTag(name) => {
                self.plan.add_tag(name);
                PlanResponse::PlanUpdated
            }

            PlanRequest::RenameTag(id, new_name) => {
                self.plan.rename_tag(&id, &new_name);
                PlanResponse::PlanUpdated
            }

            PlanRequest::DeleteTag(id) => {
                self.plan.remove_tag(&id);
                PlanResponse::PlanUpdated
            }

            PlanRequest::MoveTag(id, new_index) => {
                self.plan.move_tag(&id, new_index);
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdatePlanSettings {
                name,
                start_date,
                scheduler_target,
            } => {
                self.plan.name = name;
                self.plan.start_date = start_date;
                self.plan.scheduler_target = scheduler_target;
                PlanResponse::PlanUpdated
            }
        }
    }
}

// ── Patch application (free functions to avoid borrow issues in closures) ─────

pub(crate) fn apply_task_patch(
    plan: &mut Plan,
    id: TaskId,
    patch: TaskPatch,
) -> Result<(), PlanError> {
    if !plan.tasks.contains_key(&id) {
        return Err(PlanError::TaskNotFound(id));
    }

    // Dependencies are applied first via the plan's cycle-checking method.
    // On failure the old list is restored and the patch is rejected.
    if let Some(new_deps) = patch.dependencies {
        let old_deps = plan.tasks[&id].dependencies.clone();
        plan.tasks.get_mut(&id).unwrap().dependencies.clear();
        for dep in new_deps {
            if let Err(e) = plan.add_task_dependency(id, dep) {
                plan.tasks.get_mut(&id).unwrap().dependencies = old_deps;
                return Err(PlanError::Dependency(e));
            }
        }
    }

    let task = plan.tasks.get_mut(&id).unwrap();
    if let Some(v) = patch.name {
        task.name = v;
    }
    if let Some(v) = patch.description {
        task.description = v;
    }
    if let Some(v) = patch.constraint {
        task.constraint = v;
    }
    if let Some(v) = patch.duration_days_target {
        task.duration_days_target = v;
    }
    if let Some(v) = patch.workers {
        task.workers = v;
    }

    if let Some(v) = patch.status {
        plan.node_allocations
            .tasks
            .entry(id)
            .or_insert_with(crate::data::TaskState::not_started)
            .status = v;
    }
    if let Some(v) = patch.actual_start_date {
        plan.set_task_actual_start(id, v);
    }
    if let Some(v) = patch.actual_end_date {
        plan.set_task_actual_end(id, v);
    }

    plan.node_allocations.invalidate();
    Ok(())
}

pub(crate) fn apply_milestone_patch(
    plan: &mut Plan,
    id: MilestoneId,
    patch: MilestonePatch,
) -> Result<(), PlanError> {
    if !plan.milestones.contains_key(&id) {
        return Err(PlanError::MilestoneNotFound(id));
    }

    if let Some(new_deps) = patch.dependencies {
        let old_deps = plan.milestones[&id].dependencies.clone();
        plan.milestones.get_mut(&id).unwrap().dependencies.clear();
        for dep in new_deps {
            if let Err(e) = plan.add_milestone_dependency(id, dep) {
                plan.milestones.get_mut(&id).unwrap().dependencies = old_deps;
                return Err(PlanError::Dependency(e));
            }
        }
    }

    let ms = plan.milestones.get_mut(&id).unwrap();
    if let Some(v) = patch.name {
        ms.name = v;
    }
    if let Some(v) = patch.description {
        ms.description = v;
    }
    if let Some(v) = patch.constraint {
        ms.constraint = v;
    }

    plan.node_allocations.invalidate();
    Ok(())
}

// ── Debug helper ──────────────────────────────────────────────────────────────

/// Print a concise summary of the plan to stderr every time a mutation
/// succeeds.  Useful for diagnosing scheduler or rendering issues.
///
/// Output goes to stderr so it doesn't interfere with any stdout consumers.
/// Remove or gate behind a feature flag once the issues are resolved.
fn debug_print_plan(plan: &Plan) {
    use crate::data::ids::NodeId;
    use crate::data::schedule::Weekday;
    use crate::data::task::WorkerSlot;

    let sep = "══════════════════════════════════════════";
    eprintln!("\n{sep}");
    eprintln!("  Plan:  {:?}", plan.name);
    eprintln!("  Start: {}  scheduler_target: {}", plan.start_date, {
        match plan.scheduler_target {
            NodeId::PlanStart => "PlanStart".into(),
            NodeId::Task(tid) => plan
                .tasks
                .get(&tid)
                .map(|t| format!("Task({:?})", t.name))
                .unwrap_or_else(|| format!("Task({})", tid.0)),
            NodeId::Milestone(mid) => plan
                .milestones
                .get(&mid)
                .map(|m| format!("MS({:?})", m.name))
                .unwrap_or_else(|| format!("MS({})", mid.0)),
        }
    });

    let fmt_node = |id: &NodeId| -> String {
        match id {
            NodeId::PlanStart => "PlanStart".into(),
            NodeId::Task(tid) => plan
                .tasks
                .get(tid)
                .map(|t| format!("Task({:?})", t.name))
                .unwrap_or_else(|| format!("Task({})", tid.0)),
            NodeId::Milestone(mid) => plan
                .milestones
                .get(mid)
                .map(|m| format!("MS({:?})", m.name))
                .unwrap_or_else(|| format!("MS({})", mid.0)),
        }
    };

    let fmt_schedule = |sched: &crate::data::WorkSchedule| -> String {
        let days = [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ];
        let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let parts: Vec<String> = days
            .iter()
            .zip(day_names.iter())
            .filter_map(|(d, name)| {
                let h = sched.hours_on(*d);
                if h > 0.0 {
                    Some(format!("{name}={h:.1}h"))
                } else {
                    None
                }
            })
            .collect();
        format!(
            "[{}]  hpd={:.1}h  total={:.1}h/wk",
            parts.join("  "),
            sched.hours_per_workload_day(),
            sched.total_hours_per_week()
        )
    };

    if !plan.tags.is_empty() {
        eprintln!("  Tags ({}):", plan.tags.len());
        for tag in &plan.tags {
            eprintln!("    {:?} ({})", tag.name, tag.id.0);
        }
    }

    eprintln!(
        "  Default schedule: {}",
        fmt_schedule(&plan.default_schedule)
    );

    if !plan.calendar.entries.is_empty() {
        let mut entries: Vec<_> = plan.calendar.entries.iter().collect();
        entries.sort_by_key(|(d, _)| *d);
        eprintln!("  Plan calendar overrides ({}):", entries.len());
        for (date, hours) in entries {
            eprintln!("    {date} → {hours:.1}h");
        }
    }

    eprintln!("  Users ({}):", plan.users_data.len());
    let mut users: Vec<_> = plan.users_data.values().map(|ud| &ud.user).collect();
    users.sort_by(|a, b| a.name.cmp(&b.name));
    for user in users {
        let uid = user.id;
        let tag_names: Vec<&str> = user
            .tags
            .iter()
            .filter_map(|tid| {
                plan.tags
                    .iter()
                    .find(|t| t.id == *tid)
                    .map(|t| t.name.as_str())
            })
            .collect();
        let tags_str = if tag_names.is_empty() {
            String::new()
        } else {
            format!("  tags=[{}]", tag_names.join(", "))
        };

        if let Some(sched) = plan
            .users_data
            .get(&uid)
            .and_then(|ud| ud.schedule.as_ref())
        {
            eprintln!("    {:?}{tags_str}", user.name);
            eprintln!("      schedule: {}", fmt_schedule(sched));
        } else {
            eprintln!("    {:?}{tags_str}  schedule: (default)", user.name);
        }

        if let Some(cal) = plan.user_calendar_overrides.get(&uid)
            && !cal.entries.is_empty()
        {
            let mut entries: Vec<_> = cal.entries.iter().collect();
            entries.sort_by_key(|(d, _)| *d);
            eprintln!("      calendar overrides ({}):", entries.len());
            for (date, hours) in entries {
                eprintln!("        {date} → {hours:.1}h");
            }
        }
    }

    eprintln!("  Tasks ({}):", plan.tasks.len());
    let mut tasks: Vec<_> = plan.tasks.iter().collect();
    tasks.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
    for (tid, task) in tasks {
        let sched_start = plan
            .node_allocations
            .tasks
            .get(tid)
            .map(|ts| ts.allocation.start_date());
        let status = plan.task_status(tid);
        let duration = task.effective_duration_days();

        let deps: Vec<String> = task
            .dependencies
            .iter()
            .map(|d| {
                if d.lag_days == 0.0 {
                    fmt_node(&d.id)
                } else {
                    format!("{}(lag={:.1})", fmt_node(&d.id), d.lag_days)
                }
            })
            .collect();

        let workers: Vec<String> = task
            .workers
            .iter()
            .map(|slot| match slot {
                WorkerSlot::Specific {
                    user_id,
                    workload_days,
                } => {
                    let name = plan
                        .users_data
                        .get(user_id)
                        .map(|ud| ud.user.name.as_str())
                        .unwrap_or("?");
                    format!("{name}={workload_days:.2}wd")
                }
                WorkerSlot::Placeholder {
                    required_tags,
                    workload_days,
                } => {
                    let tag_names: Vec<&str> = required_tags
                        .iter()
                        .filter_map(|tid| {
                            plan.tags
                                .iter()
                                .find(|t| t.id == *tid)
                                .map(|t| t.name.as_str())
                        })
                        .collect();
                    format!("placeholder={workload_days:.2}wd({})", tag_names.join("+"))
                }
            })
            .collect();

        eprintln!(
            "    {:?}  dur_target={:.1}d  eff_dur={duration:.1}d  status={:?}  sched_start={:?}",
            task.name, task.duration_days_target, status, sched_start
        );
        if !workers.is_empty() {
            eprintln!("      workers: {}", workers.join(", "));
        } else {
            eprintln!("      workers: (none — pure calendar block)");
        }
        if !deps.is_empty() {
            eprintln!("      deps: {}", deps.join(", "));
        }
        if let Some(c) = &task.constraint {
            eprintln!("      constraint: {:?} on {}", c.kind, c.date);
        }
        if let Some(start) = plan.task_actual_start(tid) {
            eprintln!("      actual_start: {start}");
        }
        if let Some(end) = plan.task_actual_end(tid) {
            eprintln!("      actual_end: {end}");
        }
        if !task.description.is_empty() {
            let preview: String = task.description.chars().take(80).collect();
            eprintln!("      description: {preview:?}...");
        }
    }

    eprintln!("  Milestones ({}):", plan.milestones.len());
    let mut milestones: Vec<_> = plan.milestones.iter().collect();
    milestones.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
    for (mid, ms) in milestones {
        let sched_date = plan
            .node_allocations
            .milestones
            .get(mid)
            .map(|ma| ma.date());
        let deps: Vec<String> = ms
            .dependencies
            .iter()
            .map(|d| {
                if d.lag_days == 0.0 {
                    fmt_node(&d.id)
                } else {
                    format!("{}(lag={:.1})", fmt_node(&d.id), d.lag_days)
                }
            })
            .collect();
        eprintln!("    {:?}  sched={:?}", ms.name, sched_date);
        if !deps.is_empty() {
            eprintln!("      deps: {}", deps.join(", "));
        }
        if let Some(c) = &ms.constraint {
            eprintln!("      constraint: {:?} on {}", c.kind, c.date);
        }
    }

    let alloc = &plan.node_allocations;
    if !alloc.has_schedule() {
        eprintln!("  Allocation: None (scheduler has not run)");
    } else {
        eprintln!(
            "  Allocation: {} tasks, {} milestones",
            alloc.tasks.len(),
            alloc.milestones.len()
        );
        let mut task_allocs: Vec<_> = alloc.tasks.iter().collect();
        task_allocs.sort_by(|(a, _), (b, _)| {
            let na = plan.tasks.get(a).map(|t| t.name.as_str()).unwrap_or("");
            let nb = plan.tasks.get(b).map(|t| t.name.as_str()).unwrap_or("");
            na.cmp(nb)
        });
        for (tid, ts) in task_allocs {
            let name = plan.tasks.get(tid).map(|t| t.name.as_str()).unwrap_or("?");
            let start = ts.allocation.start_date();
            let end = ts.allocation.end_date();
            let span_days = (end - start).num_days() + 1;
            eprintln!(
                "    task {:?}: {} → {}  ({} calendar day{})",
                name,
                start,
                end,
                span_days,
                if span_days == 1 { "" } else { "s" }
            );
        }
        let mut ms_allocs: Vec<_> = alloc.milestones.iter().collect();
        ms_allocs.sort_by(|(a, _), (b, _)| {
            let na = plan
                .milestones
                .get(a)
                .map(|m| m.name.as_str())
                .unwrap_or("");
            let nb = plan
                .milestones
                .get(b)
                .map(|m| m.name.as_str())
                .unwrap_or("");
            na.cmp(nb)
        });
        for (mid, ma) in ms_allocs {
            let name = plan
                .milestones
                .get(mid)
                .map(|m| m.name.as_str())
                .unwrap_or("?");
            eprintln!("    milestone {:?}: {}", name, ma.date());
        }
    }

    eprintln!("{sep}\n");
}
