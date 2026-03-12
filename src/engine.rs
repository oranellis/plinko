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
use crate::data::ids::{MilestoneId, TagId, TaskId, UserId};
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
    pub status: Option<crate::data::task::TaskStatus>,
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
    pub fn status(mut self, v: crate::data::task::TaskStatus) -> Self {
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
    pub fn avatar(mut self, v: Option<Vec<u8>>) -> Self {
        self.avatar = Some(v);
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
    /// Update top-level plan metadata (name and start date).
    UpdatePlanSettings {
        name: String,
        start_date: chrono::NaiveDate,
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
    pub fn process_pending(&mut self) -> Vec<PlanResponse> {
        let mut responses = Vec::new();
        while let Ok(request) = self.rx.try_recv() {
            responses.push(self.process(request));
        }
        responses
    }

    // ── Validation helper ─────────────────────────────────────────────────────

    /// Apply a mutation `f` to the plan, then validate by re-running the
    /// scheduler if an allocation was present before the mutation.
    ///
    /// If the scheduler fails after the mutation, the pre-mutation plan is
    /// fully restored and [`PlanError::Scheduler`] is returned.
    fn apply_validated<F>(&mut self, f: F) -> PlanResponse
    where
        F: FnOnce(&mut Plan) -> Result<(), PlanError>,
    {
        // Snapshot the plan only when there is an existing allocation to protect.
        let backup = self.plan.allocation.is_some().then(|| self.plan.clone());

        match f(&mut self.plan) {
            Err(e) => PlanResponse::Error(e),
            Ok(()) => match backup {
                None => PlanResponse::PlanUpdated,
                Some(backup_plan) => match self.plan.compute_time_optimised_plan() {
                    Ok(()) => PlanResponse::PlanUpdated,
                    Err(e) => {
                        self.plan = backup_plan;
                        PlanResponse::Error(PlanError::Scheduler(e))
                    }
                },
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
            PlanRequest::StartTask(id) => match self.plan.tasks.get_mut(&id) {
                Some(task) => {
                    task.start();
                    PlanResponse::PlanUpdated
                }
                None => PlanResponse::Error(PlanError::TaskNotFound(id)),
            },

            PlanRequest::PauseTask(id) => match self.plan.tasks.get_mut(&id) {
                Some(task) => {
                    task.pause();
                    PlanResponse::PlanUpdated
                }
                None => PlanResponse::Error(PlanError::TaskNotFound(id)),
            },

            PlanRequest::ResumeTask(id) => match self.plan.tasks.get_mut(&id) {
                Some(task) => {
                    task.resume();
                    PlanResponse::PlanUpdated
                }
                None => PlanResponse::Error(PlanError::TaskNotFound(id)),
            },

            PlanRequest::CompleteTask(id) => match self.plan.tasks.get_mut(&id) {
                Some(task) => {
                    task.complete();
                    PlanResponse::PlanUpdated
                }
                None => PlanResponse::Error(PlanError::TaskNotFound(id)),
            },

            PlanRequest::DropTask(id) => match self.plan.tasks.get_mut(&id) {
                Some(task) => {
                    task.drop_task();
                    PlanResponse::PlanUpdated
                }
                None => PlanResponse::Error(PlanError::TaskNotFound(id)),
            },

            // ── Task CRUD ─────────────────────────────────────────────────────
            PlanRequest::CreateTask(task) => {
                self.plan.add_task(task);
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdateTask(id, patch) => {
                self.apply_validated(|plan| apply_task_patch(plan, id, patch))
            }

            PlanRequest::DeleteTask(id) => self.apply_validated(|plan| {
                if plan.tasks.remove(&id).is_some() {
                    plan.allocation = None;
                    Ok(())
                } else {
                    Err(PlanError::TaskNotFound(id))
                }
            }),

            // ── Milestone CRUD ────────────────────────────────────────────────
            PlanRequest::CreateMilestone(milestone) => {
                self.plan.add_milestone(milestone);
                PlanResponse::PlanUpdated
            }

            PlanRequest::UpdateMilestone(id, patch) => {
                self.apply_validated(|plan| apply_milestone_patch(plan, id, patch))
            }

            PlanRequest::DeleteMilestone(id) => self.apply_validated(|plan| {
                if plan.milestones.remove(&id).is_some() {
                    plan.allocation = None;
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
                let user = plan.users.get_mut(&id).ok_or(PlanError::UserNotFound(id))?;
                if let Some(v) = patch.name {
                    user.name = v;
                }
                if let Some(v) = patch.tags {
                    user.tags = v;
                }
                if let Some(v) = patch.avatar {
                    user.avatar = v;
                }
                plan.allocation = None;
                Ok(())
            }),

            PlanRequest::DeleteUser(id) => self.apply_validated(|plan| {
                if plan.users.remove(&id).is_some() {
                    plan.user_schedules.remove(&id);
                    plan.user_calendars.remove(&id);
                    plan.allocation = None;
                    Ok(())
                } else {
                    Err(PlanError::UserNotFound(id))
                }
            }),

            PlanRequest::SetUserSchedule(id, schedule) => self.apply_validated(|plan| {
                if !plan.users.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.set_user_schedule(id, schedule);
                Ok(())
            }),

            PlanRequest::ClearUserSchedule(id) => self.apply_validated(|plan| {
                if !plan.users.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.clear_user_schedule(&id);
                Ok(())
            }),

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

            PlanRequest::UpdatePlanSettings { name, start_date } => {
                self.plan.name = name;
                self.plan.start_date = start_date;
                PlanResponse::PlanUpdated
            }
        }
    }
}

// ── Patch application (free functions to avoid borrow issues in closures) ─────

fn apply_task_patch(plan: &mut Plan, id: TaskId, patch: TaskPatch) -> Result<(), PlanError> {
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
    if let Some(v) = patch.status {
        task.status = v;
    }
    if let Some(v) = patch.actual_start_date {
        task.actual_start_date = v;
    }
    if let Some(v) = patch.actual_end_date {
        task.actual_end_date = v;
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

    plan.allocation = None;
    Ok(())
}

fn apply_milestone_patch(
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

    plan.allocation = None;
    Ok(())
}
