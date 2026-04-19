use crate::data::constraint::DateConstraint;
use crate::data::dependency::Dependency;
use crate::data::ids::{MilestoneId, NodeId, TagId, TaskId, UserId};
use crate::data::plan::DependencyError;
use crate::data::scheduler::SchedulerError;
use crate::data::task::WorkerSlot;
use crate::data::{Milestone, Plan, Status, Task, User, WorkSchedule};
use crate::monday::{BoardColumn, MondayConfig, MondayUser};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

// ── Auth shared types ────────────────────────────────────────────────────────

/// A login user record (no password hash).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
}

/// Role within an organisation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OrgRole {
    Admin,
    User,
    Viewer,
}

/// A user's membership in an organisation.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrgMembership {
    pub org_id: String,
    pub org_name: String,
    pub role: OrgRole,
}

/// Summary of an organisation (for lists).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Organisation {
    pub id: String,
    pub name: String,
}

/// A member of an organisation with their role.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrgMember {
    pub user_id: String,
    pub email: String,
    pub role: OrgRole,
}

/// Per-plan permission entry for a specific user (used in the plan-access management UI).
/// `permission` is one of `"NoAccess"`, `"Viewer"`, `"User"`, or `"Default"` (inherit org role).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlanPermissionEntry {
    pub plan_id: uuid::Uuid,
    pub plan_name: String,
    pub permission: String,
}

/// Maps a login user UUID to a plan user UUID.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserLink {
    pub login_user_id: Uuid,
    pub plan_user_id: UserId,
}

/// Deserializes an `Option<Option<T>>` field so that an absent JSON field
/// produces `None` (no-op / don't update) while an explicit JSON `null`
/// produces `Some(None)` (clear the value).  Without this, both cases
/// round-trip as `None` because serde maps JSON `null` to the outer `None`.
fn deserialize_optional_field<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::deserialize(de)?))
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct TaskPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<Status>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub actual_start_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub actual_end_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub constraint: Option<Option<DateConstraint>>,
    pub duration_days_target: Option<f32>,
    pub workers: Option<Vec<WorkerSlot>>,
    pub dependencies: Option<Vec<Dependency>>,
    pub relaxed_mode: Option<bool>,
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
    pub fn relaxed_mode(mut self, v: bool) -> Self {
        self.relaxed_mode = Some(v);
        self
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct MilestonePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
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
    MoveUser(UserId, usize),
    UpdatePlanSettings {
        name: String,
        start_date: NaiveDate,
        scheduler_target: NodeId,
    },
    SavePlan,
    NewPlan {
        org_id: Option<String>,
    },
    LoadPlan {
        plan_id: uuid::Uuid,
    },
    DeletePlan {
        plan_id: uuid::Uuid,
    },
    ListPlans,
    SetCurrentUser(Option<UserId>),
    ListPlanVersions {
        plan_id: uuid::Uuid,
    },
    RestorePlanVersion {
        plan_id: uuid::Uuid,
        version: String,
    },
    // Monday.com integration (handled server-side)
    MondayTestConnection {
        token: String,
        board_id: String,
    },
    MondayFetchBoardInfo {
        token: String,
        board_id: String,
    },
    MondayPull {
        plan_id: uuid::Uuid,
    },
    MondayFullReimport {
        plan_id: uuid::Uuid,
    },
    MondayPush {
        plan_id: uuid::Uuid,
    },
    /// Compute what a push would change without executing it.
    /// Returns `PlanResponse::MondayPushPreview` with op and new-item counts.
    MondayPushPreview {
        plan_id: uuid::Uuid,
    },
    SaveMondayConfig {
        plan_id: uuid::Uuid,
        config: Box<MondayConfig>,
        token: String,
    },
    LoadMondayConfig {
        plan_id: uuid::Uuid,
    },
    LoadMondayApiToken,
    // Auth user management (admin-only except ChangeMyPassword/GetUserLinks/SetUserLinks)
    GetAuthUsers,
    CreateAuthUser {
        email: String,
        password: String,
        is_admin: bool,
    },
    UpdateAuthUser {
        user_id: String,
        new_email: Option<String>,
        new_is_admin: Option<bool>,
    },
    SetAuthUserPassword {
        user_id: String,
        new_password: String,
    },
    DeleteAuthUser {
        user_id: String,
    },
    ChangeMyPassword {
        old_password: String,
        new_password: String,
    },
    // Per-plan login→plan-user links
    GetUserLinks {
        plan_id: uuid::Uuid,
    },
    SetUserLinks {
        plan_id: uuid::Uuid,
        links: Vec<UserLink>,
    },
    // Organisation management
    ListOrganisations,
    CreateOrganisation {
        name: String,
    },
    DeleteOrganisation {
        org_id: String,
    },
    RenameOrganisation {
        org_id: String,
        name: String,
    },
    GetOrgMembers {
        org_id: String,
    },
    AddOrgMember {
        org_id: String,
        user_id: String,
        role: OrgRole,
    },
    RemoveOrgMember {
        org_id: String,
        user_id: String,
    },
    SetPlanOrg {
        plan_id: uuid::Uuid,
        org_id: Option<String>,
    },
    GetPlanOrg {
        plan_id: uuid::Uuid,
    },
    // Per-plan permission management (org admins / site admins only)
    GetOrgPlans {
        org_id: String,
    },
    GetUserPlanPermissions {
        org_id: String,
        user_id: String,
    },
    SetUserPlanPermission {
        plan_id: uuid::Uuid,
        user_id: String,
        /// One of "NoAccess", "Viewer", "User", or "Default" (removes the explicit override).
        permission: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlanResponse {
    PlanUpdated,
    Error(PlanError),
    PlanList(Vec<(uuid::Uuid, String, String)>),
    // Monday responses
    MondayConfigLoaded(Box<MondayConfig>),
    MondayBoardInfo {
        users: Vec<MondayUser>,
        columns: Vec<BoardColumn>,
        status_labels: Vec<String>,
    },
    MondayApiToken(String),
    MondayConnected(String),
    /// Result of `MondayPushPreview`: number of field-level update ops and
    /// number of new items that would be created on Monday.
    MondayPushPreview {
        op_count: usize,
        new_item_count: usize,
    },
    // Auth responses
    AuthUsers(Vec<AuthUser>),
    UserLinks(Vec<UserLink>),
    PasswordChanged,
    AuthUserCreated {
        user_id: String,
    },
    PlanVersionList(Vec<String>),
    OrgList(Vec<Organisation>),
    OrgCreated {
        id: String,
        name: String,
    },
    OrgMembers(Vec<OrgMember>),
    PlanOrgId(Option<String>),
    OrgPlanList(Vec<(uuid::Uuid, String)>),
    UserPlanPermissions(Vec<PlanPermissionEntry>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlanError {
    TaskNotFound(TaskId),
    MilestoneNotFound(MilestoneId),
    UserNotFound(UserId),
    Scheduler(SchedulerError),
    Dependency(DependencyError),
    Monday(String),
    Unauthorized,
    AuthError(String),
    NoPlanActive,
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
            PlanError::Monday(msg) => write!(f, "Monday.com error: {msg}"),
            PlanError::Unauthorized => write!(f, "Unauthorized — admin access required"),
            PlanError::AuthError(msg) => write!(f, "Auth error: {msg}"),
            PlanError::NoPlanActive => {
                write!(f, "No plan active — load or create a plan first")
            }
        }
    }
}

pub const VERSION: &str = "0.5.5";

/// Per-user server-side preferences.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UserPrefs {
    pub last_plan_id: Option<uuid::Uuid>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Hello {
        version: String,
    },
    VersionError {
        expected: String,
        got: String,
    },
    PlanState {
        plan: Box<Plan>,
        has_monday_integration: bool,
    },
    NoPlanActive,
    Response {
        id: u64,
        response: PlanResponse,
    },
    // Monday progress — sent unsolicited while an operation is in-flight
    MondayProgress {
        done: usize,
        total: usize,
        message: String,
    },
    MondayDone {
        message: String,
    },
    MondayError {
        message: String,
    },
    // Auth
    AuthRequired,
    LoginSuccess {
        session_token: String,
        user_id: String,
        email: String,
        is_admin: bool,
        user_prefs: UserPrefs,
        org_memberships: Vec<OrgMembership>,
    },
    LoginFailed {
        message: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Hello { version: String },
    Request { id: u64, request: PlanRequest },
    Login { email: String, password: String },
    Authenticate { session_token: String },
    Logout,
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
    // Validate new workers (if any) before mutating the task.
    if let Some(ref workers) = patch.workers {
        let task_name = plan.tasks[&id].name.clone();
        plan.validate_task_workers(&task_name, workers)
            .map_err(PlanError::Scheduler)?;
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
    if let Some(v) = patch.relaxed_mode {
        task.relaxed_mode = v;
    }
    if let Some(v) = patch.status {
        use crate::data::{TaskAllocation, TaskState};
        let ts = plan
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = v;
        // Non-NotStarted statuses must use a Fixed allocation so the status
        // survives invalidate() which purges Dynamic (scheduler-output) entries.
        if v != crate::data::Status::NotStarted
            && matches!(ts.allocation, TaskAllocation::Dynamic { .. })
        {
            let sentinel = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
            let start = plan
                .tasks
                .get(&id)
                .and_then(|t| t.actual_start)
                .unwrap_or(sentinel);
            ts.allocation = TaskAllocation::Fixed {
                start_date: start,
                end_date: start,
                corrected_end_date: None,
                time_allocation: vec![],
            };
        }
    }
    if let Some(v) = patch.actual_start_date {
        plan.set_task_actual_start(id, v);
    }
    if let Some(v) = patch.actual_end_date {
        plan.set_task_actual_end(id, v);
    }
    plan.node_allocations.invalidate();
    plan.simplify_all_dependencies();
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
    plan.simplify_all_dependencies();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_constraint_via_json_roundtrip() {
        // Some(None) must survive JSON serialisation so the server can
        // distinguish "clear constraint" from "no change".
        let patch = MilestonePatch::new().constraint(None);
        let json = serde_json::to_string(&patch).unwrap();
        let decoded: MilestonePatch = serde_json::from_str(&json).unwrap();
        assert!(
            decoded.constraint == Some(None),
            "constraint should round-trip as Some(None) (clear), got {:?}",
            decoded.constraint
        );

        let patch2 = TaskPatch::new().constraint(None);
        let json2 = serde_json::to_string(&patch2).unwrap();
        let decoded2: TaskPatch = serde_json::from_str(&json2).unwrap();
        assert!(
            decoded2.constraint == Some(None),
            "constraint should round-trip as Some(None) (clear), got {:?}",
            decoded2.constraint
        );
    }
}
