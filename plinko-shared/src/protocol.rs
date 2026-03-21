use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::{MilestoneId, NodeId, TagId, TaskId, UserId};
use crate::data::plan::DependencyError;
use crate::data::scheduler::SchedulerError;
use crate::data::task::WorkerSlot;
use crate::data::{Milestone, Plan, Status, Task, User, WorkSchedule};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct TaskPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<Status>,
    pub actual_start_date: Option<Option<NaiveDate>>,
    pub actual_end_date: Option<Option<NaiveDate>>,
    pub constraint: Option<Option<DateConstraint>>,
    pub duration_days_target: Option<f32>,
    pub workers: Option<Vec<WorkerSlot>>,
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
    pub fn status(mut self, v: Status) -> Self {
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

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct MilestonePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub constraint: Option<Option<DateConstraint>>,
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

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct UserPatch {
    pub name: Option<String>,
    pub tags: Option<HashSet<TagId>>,
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlanRequest {
    RunScheduler,
    StartTask(TaskId),
    PauseTask(TaskId),
    ResumeTask(TaskId),
    CompleteTask(TaskId),
    DropTask(TaskId),
    CreateTask(Task),
    UpdateTask(TaskId, TaskPatch),
    DeleteTask(TaskId),
    CreateMilestone(Milestone),
    UpdateMilestone(MilestoneId, MilestonePatch),
    DeleteMilestone(MilestoneId),
    CreateUser(User),
    UpdateUser(UserId, UserPatch),
    DeleteUser(UserId),
    SetUserSchedule(UserId, WorkSchedule),
    ClearUserSchedule(UserId),
    SetDefaultSchedule(WorkSchedule),
    SetCalendarOverride(NaiveDate, f32),
    ClearCalendarOverride(NaiveDate),
    SetUserCalendarOverride(UserId, NaiveDate, f32),
    ClearUserCalendarOverride(UserId, NaiveDate),
    ReplacePlan(Box<Plan>),
    AddTag(String),
    RenameTag(TagId, String),
    DeleteTag(TagId),
    MoveTag(TagId, usize),
    UpdatePlanSettings {
        name: String,
        start_date: NaiveDate,
        scheduler_target: NodeId,
    },
    SavePlan,
    NewPlan,
    LoadPlan {
        plan_id: uuid::Uuid,
    },
    ListPlans,
    SetCurrentUser(Option<UserId>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlanResponse {
    PlanUpdated,
    Error(PlanError),
    PlanList(Vec<(uuid::Uuid, String, String)>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlanError {
    TaskNotFound(TaskId),
    MilestoneNotFound(MilestoneId),
    UserNotFound(UserId),
    Scheduler(SchedulerError),
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

pub const VERSION: &str = "0.1";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Hello { version: String },
    VersionError { expected: String, got: String },
    PlanState { plan: Box<Plan> },
    Response { id: u64, response: PlanResponse },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Hello { version: String },
    Request { id: u64, request: PlanRequest },
}

/// Apply a task patch to the plan (used for dry-run validation in the UI and
/// for actual mutations in the server engine).
pub fn apply_task_patch(plan: &mut Plan, id: TaskId, patch: TaskPatch) -> Result<(), PlanError> {
    if !plan.tasks.contains_key(&id) {
        return Err(PlanError::TaskNotFound(id));
    }
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

/// Apply a milestone patch to the plan.
pub fn apply_milestone_patch(
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
