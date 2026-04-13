/**
 * TypeScript mirror of `plinko-shared/src/protocol.rs` and associated data types.
 *
 * Serialisation rules:
 *  - Rust tuple-newtype IDs (e.g. `TaskId(Uuid)`) serialize as plain UUID strings.
 *  - Rust enums with `#[serde(tag = "type")]` become discriminated unions keyed on `type`.
 *  - Rust enums *without* a tag attribute use the default serde encoding:
 *      unit variants  → bare string  (e.g. `"PlanStart"`)
 *      struct/tuple variants → `{ "VariantName": { ...fields } }`
 *  - `HashMap<K, V>` with string-serialisable keys → `Record<string, V>`.
 *  - `NaiveDate` → ISO-8601 date string (`"YYYY-MM-DD"`).
 *  - `Option<T>` → `T | null` in JSON; `undefined` means the field is absent.
 */

// ── Primitive aliases ────────────────────────────────────────────────────────

export type TaskId = string;       // UUID
export type MilestoneId = string;  // UUID
export type UserId = string;       // UUID
export type TagId = string;        // UUID
export type IsoDate = string;      // "YYYY-MM-DD"

// ── NodeId ───────────────────────────────────────────────────────────────────
// Serde default (no tag): unit variant is bare string, struct variants are wrapped.
export type NodeId =
  | "PlanStart"
  | { Task: TaskId }
  | { Milestone: MilestoneId };

// ── Constraint ───────────────────────────────────────────────────────────────

export type ConstraintKind = "Fixed" | "Earliest" | "Latest";

export interface DateConstraint {
  date: IsoDate;
  kind: ConstraintKind;
}

// ── Dependency ───────────────────────────────────────────────────────────────

export interface Dependency {
  id: NodeId;
  lag_days: number;
}

// ── Worker slots ─────────────────────────────────────────────────────────────
// Serde default enum encoding: { "Specific": { user_id, workload_days } }

export type WorkerSlot =
  | { Specific: { user_id: UserId; workload_days: number } }
  | { Placeholder: { required_tags: TagId[]; workload_days: number } };

// ── Status ───────────────────────────────────────────────────────────────────

export type Status =
  | "NotStarted"
  | "InProgress"
  | "OnHold"
  | "Complete"
  | "Dropped";

// ── Task ─────────────────────────────────────────────────────────────────────

export interface Task {
  id: TaskId;
  name: string;
  description: string;
  dependencies: Dependency[];
  workers: WorkerSlot[];
  constraint: DateConstraint | null;
  duration_days_target: number;
  relaxed_mode: boolean;
  actual_start: IsoDate | null;
  context_label: string | null;
}

// ── Milestone ────────────────────────────────────────────────────────────────

export interface Milestone {
  id: MilestoneId;
  name: string;
  description: string;
  dependencies: Dependency[];
  constraint: DateConstraint | null;
  context_label: string | null;
}

// ── Tag ──────────────────────────────────────────────────────────────────────

export interface Tag {
  id: TagId;
  name: string;
}

// ── User ─────────────────────────────────────────────────────────────────────

export interface User {
  id: UserId;
  name: string;
  tags: TagId[];
}

// ── WorkSchedule ─────────────────────────────────────────────────────────────
// Weekday enum serialises as string.

export type Weekday =
  | "Monday"
  | "Tuesday"
  | "Wednesday"
  | "Thursday"
  | "Friday"
  | "Saturday"
  | "Sunday";

export interface WorkSchedule {
  days: Partial<Record<Weekday, number>>;
}

// ── UserData ─────────────────────────────────────────────────────────────────

export interface UserData {
  user: User;
  schedule: WorkSchedule | null;
}

// ── CalendarOverrides ────────────────────────────────────────────────────────

export interface CalendarOverrides {
  entries: Record<IsoDate, number>;
}

// ── Allocation types ─────────────────────────────────────────────────────────

export interface WorkSegment {
  user: UserId;
  date: IsoDate;
  hours_worked: number;
}

export type TaskAllocation =
  | {
      Fixed: {
        start_date: IsoDate;
        end_date: IsoDate;
        corrected_end_date: IsoDate | null;
        time_allocation: WorkSegment[];
      };
    }
  | {
      Dynamic: {
        scheduled_start_date: IsoDate;
        scheduled_end_date: IsoDate;
        time_allocation: WorkSegment[];
      };
    };

export interface TaskState {
  status: Status;
  allocation: TaskAllocation;
}

export interface MilestoneAllocation {
  date: IsoDate;
  derived_status: Status;
}

export interface ConstraintViolation {
  node_name: string;
  kind: ConstraintKind;
  required_date: IsoDate;
  scheduled_date: IsoDate;
}

// NodeAllocations: constraint_violations uses NodeId-string keys
export interface NodeAllocations {
  tasks: Record<TaskId, TaskState>;
  milestones: Record<MilestoneId, MilestoneAllocation>;
  constraint_violations: Record<string, ConstraintViolation>;
}

// ── Plan ─────────────────────────────────────────────────────────────────────

export interface Plan {
  id: string; // Uuid
  name: string;
  users_data: Record<UserId, UserData>;
  user_order: UserId[];
  tags: Tag[];
  tasks: Record<TaskId, Task>;
  milestones: Record<MilestoneId, Milestone>;
  start_date: IsoDate;
  default_schedule: WorkSchedule;
  calendar: CalendarOverrides;
  user_calendar_overrides: Record<UserId, CalendarOverrides>;
  scheduler_target: NodeId;
  node_allocations: NodeAllocations;
}

// ── Monday config types ───────────────────────────────────────────────────────

export interface ColumnMap {
  person_column_id: string;
  status_column_id: string;
  dependency_column_id: string;
  workload_column_id: string;
  timeline_column_id: string;
}

export interface UserMapping {
  monday_user_id: string;
  monday_name: string;
  plinko_user_id: UserId | null;
}

export interface StatusMapping {
  monday_label: string;
  plinko_status: Status;
}

export interface ItemNodeMapping {
  monday_item_id: string;
  plinko_node_id: NodeId;
  board_id: string;
}

export interface MondayConfig {
  board_id: string;
  column_map: ColumnMap;
  user_mappings: UserMapping[];
  status_mappings: StatusMapping[];
  item_node_map: ItemNodeMapping[];
  use_subitems: boolean;
  workload_in_hours: boolean;
  show_monday_context: boolean;
}

export interface BoardColumn {
  id: string;
  title: string;
  column_type: string;
}

export interface MondayUser {
  id: string;
  name: string;
  email: string;
}

// ── Protocol: patches ────────────────────────────────────────────────────────

export interface TaskPatch {
  name?: string;
  description?: string;
  status?: Status;
  actual_start_date?: IsoDate | null;
  actual_end_date?: IsoDate | null;
  constraint?: DateConstraint | null;
  duration_days_target?: number;
  workers?: WorkerSlot[];
  dependencies?: Dependency[];
  relaxed_mode?: boolean;
}

export interface MilestonePatch {
  name?: string;
  description?: string;
  constraint?: DateConstraint | null;
  dependencies?: Dependency[];
}

export interface UserPatch {
  name?: string;
  tags?: TagId[];
}

// ── Auth types ────────────────────────────────────────────────────────────────

export interface AuthUser {
  id: string;
  email: string;
  is_admin: boolean;
}

export interface UserLink {
  login_user_id: string;
  plan_user_id: UserId;
}

// ── Protocol: PlanRequest ─────────────────────────────────────────────────────
// Serde default enum encoding. Unit variants are bare strings;
// tuple/struct variants are `{ "VariantName": payload }`.

export type PlanRequest =
  | "RunScheduler"
  | "SavePlan"
  | "NewPlan"
  | "ListPlans"
  | { StartTask: TaskId }
  | { PauseTask: TaskId }
  | { ResumeTask: TaskId }
  | { CompleteTask: TaskId }
  | { DropTask: TaskId }
  | { CreateTask: Task }
  | { UpdateTask: [TaskId, TaskPatch] }
  | { DeleteTask: TaskId }
  | { CreateMilestone: Milestone }
  | { UpdateMilestone: [MilestoneId, MilestonePatch] }
  | { DeleteMilestone: MilestoneId }
  | { CreateUser: User }
  | { UpdateUser: [UserId, UserPatch] }
  | { DeleteUser: UserId }
  | { SetUserSchedule: [UserId, WorkSchedule] }
  | { ClearUserSchedule: UserId }
  | { SetDefaultSchedule: WorkSchedule }
  | { SetCalendarOverride: [IsoDate, number] }
  | { ClearCalendarOverride: IsoDate }
  | { SetUserCalendarOverride: [UserId, IsoDate, number] }
  | { ClearUserCalendarOverride: [UserId, IsoDate] }
  | { ReplacePlan: Plan }
  | { AddTag: string }
  | { RenameTag: [TagId, string] }
  | { DeleteTag: TagId }
  | { MoveTag: [TagId, number] }
  | { MoveUser: [UserId, number] }
  | { UpdatePlanSettings: { name: string; start_date: IsoDate; scheduler_target: NodeId } }
  | { LoadPlan: { plan_id: string } }
  | { DeletePlan: { plan_id: string } }
  | { SetCurrentUser: UserId | null }
  | { MondayTestConnection: { token: string; board_id: string } }
  | { MondayFetchBoardInfo: { token: string; board_id: string } }
  | { MondayPull: { plan_id: string } }
  | { MondayFullReimport: { plan_id: string } }
  | { MondayPush: { plan_id: string } }
  | { SaveMondayConfig: { plan_id: string; config: MondayConfig; token: string } }
  | { LoadMondayConfig: { plan_id: string } }
  | "LoadMondayApiToken"
  // Auth
  | "GetAuthUsers"
  | { CreateAuthUser: { email: string; password: string; is_admin: boolean } }
  | { UpdateAuthUser: { user_id: string; new_email?: string; new_is_admin?: boolean } }
  | { SetAuthUserPassword: { user_id: string; new_password: string } }
  | { DeleteAuthUser: { user_id: string } }
  | { ChangeMyPassword: { old_password: string; new_password: string } }
  | { GetUserLinks: { plan_id: string } }
  | { SetUserLinks: { plan_id: string; links: UserLink[] } }
  | { GetPlanVisibility: { plan_id: string } }
  | { SetPlanVisibility: { plan_id: string; user_ids: string[] } }
  | { ListPlanVersions: { plan_id: string } }
  | { RestorePlanVersion: { plan_id: string; version: string } };

// ── Protocol: PlanResponse ────────────────────────────────────────────────────

export type PlanResponse =
  | "PlanUpdated"
  | "PasswordChanged"
  | { Error: PlanError }
  | { PlanList: [string, string, string][] }
  | { MondayConfigLoaded: MondayConfig }
  | { MondayBoardInfo: { users: MondayUser[]; columns: BoardColumn[]; status_labels: string[] } }
  | { MondayApiToken: string }
  | { MondayConnected: string }
  | { AuthUsers: AuthUser[] }
  | { UserLinks: UserLink[] }
  | { AuthUserCreated: { user_id: string } }
  | { PlanVisibility: { plan_id: string; user_ids: string[] } }
  | { PlanVersionList: string[] };

export type SchedulerError =
  | "EmptyChain"
  | { MissingTaskAffinity: { task_name: string; required_tags: string[] } }
  | { SpecificWorkerNotFound: { task_name: string; user_id: string } }
  | { NoPathsToNode: unknown }
  | { DisconnectedNode: unknown };

export type PlanError =
  | { TaskNotFound: TaskId }
  | { MilestoneNotFound: MilestoneId }
  | { UserNotFound: UserId }
  | { Scheduler: SchedulerError }
  | { Dependency: "Cycle" | "NotFound" }
  | { Monday: string }
  | "Unauthorized"
  | { AuthError: string }
  | "NoPlanActive";

export function formatPlanError(err: PlanError): string {
  if (typeof err === "string") return err; // "Unauthorized", "NoPlanActive"
  if ("TaskNotFound" in err) return "Task not found.";
  if ("MilestoneNotFound" in err) return "Milestone not found.";
  if ("UserNotFound" in err) return "User not found.";
  if ("Dependency" in err)
    return err.Dependency === "Cycle"
      ? "Adding this dependency would create a cycle."
      : "Dependency not found.";
  if ("Monday" in err) return `Monday.com error: ${err.Monday}`;
  if ("AuthError" in err) return err.AuthError;
  if ("Scheduler" in err) {
    const se = err.Scheduler;
    if (se === "EmptyChain") return "Scheduler error: empty node chain.";
    if (typeof se === "object" && "MissingTaskAffinity" in se) {
      const { task_name, required_tags } = se.MissingTaskAffinity;
      if (required_tags.length === 0)
        return `Cannot create task "${task_name}": no users exist in this plan. Add a user first.`;
      return `Cannot create task "${task_name}": no user has all required tags (${required_tags.join(", ")}).`;
    }
    if (typeof se === "object" && "SpecificWorkerNotFound" in se) {
      return `Cannot create task "${se.SpecificWorkerNotFound.task_name}": a selected worker is not in this plan.`;
    }
    if (typeof se === "object" && "DisconnectedNode" in se) {
      return "One or more dependencies reference a node that doesn't exist in the plan.";
    }
    return `Scheduler error: ${JSON.stringify(se)}`;
  }
  return JSON.stringify(err);
}

export interface UserPrefs {
  last_plan_id: string | null;
}

// ── Protocol: ServerMessage (`#[serde(tag = "type")]`) ───────────────────────

export type ServerMessage =
  | { type: "Hello"; version: string }
  | { type: "VersionError"; expected: string; got: string }
  | { type: "PlanState"; plan: Plan; has_monday_integration: boolean }
  | { type: "NoPlanActive" }
  | { type: "Response"; id: number; response: PlanResponse }
  | { type: "MondayProgress"; done: number; total: number; message: string }
  | { type: "MondayDone"; message: string }
  | { type: "MondayError"; message: string }
  | { type: "AuthRequired" }
  | { type: "LoginSuccess"; session_token: string; user_id: string; email: string; is_admin: boolean; user_prefs: UserPrefs }
  | { type: "LoginFailed"; message: string };

// ── Protocol: ClientMessage (`#[serde(tag = "type")]`) ───────────────────────

export type ClientMessage =
  | { type: "Hello"; version: string }
  | { type: "Request"; id: number; request: PlanRequest }
  | { type: "Login"; email: string; password: string }
  | { type: "Authenticate"; session_token: string }
  | { type: "Logout" };

// ── Protocol version ─────────────────────────────────────────────────────────

export const PROTOCOL_VERSION = "0.3.4";
