# Plinko UI Reference Document

This document describes every screen, floating window, text input, toolbar button, and
associated server API call in the current Plinko desktop application. It is intended as the
authoritative reference for the React migration.

---

## Architecture Overview

The current app is a Rust desktop app:

```
plinko (binary)
  ├── plinko/src/main.rs         — entry point; spins up server + UI
  ├── plinko/src/server.rs       — TCP server; handles PlanRequests, broadcasts PlanState
  ├── plinko/src/engine.rs       — PlanEngine: applies requests, runs scheduler
  ├── plinko-shared/src/         — data model, protocol types, scheduler, storage
  │   ├── protocol.rs            — PlanRequest / PlanResponse / ServerMessage / ClientMessage
  │   └── monday/config.rs       — MondayConfig, MondayItem, ColumnMap, etc.
  └── plinko-ui/src/             — Skia/OpenGL GUI
      ├── app.rs                 — Application event loop, page navigation, engine messages
      ├── engine.rs              — NetworkEngine: TCP client connecting to server
      ├── pages/                 — Six pages (Home, Daily, Overview, Allocation, CalendarOverrides, Settings)
      ├── ui/                    — Floating windows, shared widgets (TextInput, MultiLineInput)
      └── monday/                — Monday.com HTTP client, import, export (UI-side, to be moved)
```

### Protocol

The UI connects to the server over **localhost TCP** using **newline-delimited JSON**.

Server → Client messages (`ServerMessage`):
- `Hello { version }` — handshake
- `VersionError { expected, got }`
- `PlanState { plan: Plan }` — full snapshot broadcast after every mutation
- `Response { id, response: PlanResponse }` — reply to a specific request

Client → Server messages (`ClientMessage`):
- `Hello { version }`
- `Request { id, request: PlanRequest }`

`PlanRequest` variants (full list):
```
RunScheduler
StartTask(TaskId), PauseTask(TaskId), ResumeTask(TaskId), CompleteTask(TaskId), DropTask(TaskId)
CreateTask(Task), UpdateTask(TaskId, TaskPatch), DeleteTask(TaskId)
CreateMilestone(Milestone), UpdateMilestone(MilestoneId, MilestonePatch), DeleteMilestone(MilestoneId)
CreateUser(User), UpdateUser(UserId, UserPatch), DeleteUser(UserId)
SetUserSchedule(UserId, WorkSchedule), ClearUserSchedule(UserId), SetDefaultSchedule(WorkSchedule)
SetCalendarOverride(NaiveDate, f32), ClearCalendarOverride(NaiveDate)
SetUserCalendarOverride(UserId, NaiveDate, f32), ClearUserCalendarOverride(UserId, NaiveDate)
ReplacePlan(Plan)
AddTag(String), RenameTag(TagId, String), DeleteTag(TagId), MoveTag(TagId, usize)
UpdatePlanSettings { name, start_date, scheduler_target: NodeId }
SavePlan, NewPlan, LoadPlan { plan_id }, DeletePlan { plan_id }, ListPlans
SetCurrentUser(Option<UserId>)
```

`PlanResponse` variants:
```
PlanUpdated         — mutation succeeded; full PlanState broadcast follows
Error(PlanError)    — TaskNotFound, MilestoneNotFound, UserNotFound, Scheduler, Dependency
PlanList(Vec<(Uuid, String, String)>)  — (id, name, timestamp)
```

---

## Data Model Summary

All data lives in `Plan`:

| Field | Type | Description |
|---|---|---|
| `id` | `Uuid` | Unique plan identifier |
| `name` | `String` | Plan display name |
| `start_date` | `NaiveDate` | Root anchor for scheduling |
| `scheduler_target` | `NodeId` | Node to optimise toward (Task, Milestone, or PlanStart) |
| `tasks` | `HashMap<TaskId, Task>` | All tasks |
| `milestones` | `HashMap<MilestoneId, Milestone>` | All milestones |
| `users_data` | `HashMap<UserId, UserData>` | Users + per-user schedule |
| `tags` | `Vec<(TagId, String)>` | Ordered tags |
| `default_schedule` | `WorkSchedule` | Hours per weekday (plan-wide) |
| `calendar` | `CalendarOverrides` | Per-date hour exceptions (plan-wide) |
| `user_calendar_overrides` | `HashMap<UserId, CalendarOverrides>` | Per-user exceptions |
| `node_allocations` | `NodeAllocations` | Scheduler output (start/end dates, time segments) |

**Task** fields: `name`, `description`, `workers: Vec<WorkerSlot>`, `duration_days_target: f32`,
`dependencies: Vec<Dependency>`, `required_tags`, `constraint: Option<DateConstraint>`,
`relaxed_mode: bool`, `actual_start: Option<NaiveDate>`, `actual_end: Option<NaiveDate>`,
`context_label: Option<String>`.

**Milestone** fields: `name`, `description`, `dependencies: Vec<Dependency>`,
`constraint: Option<DateConstraint>`, `context_label: Option<String>`.

**Dependency**: `id: DependencyId` (Task, Milestone, or PlanStart), `lag_days: f32`.

**DateConstraint**: `kind` (Earliest | Fixed | Latest), `date: NaiveDate`.

---

## Pages

All pages share the same top toolbar strip (back-arrow ← at top-left, then page-specific icon buttons).
Clicking ← navigates back to the Home screen.

### 1. Home Page

**Function:** Navigation hub. No plan data displayed.

**Layout:**
- Dark background (`#252526`)
- 5 rounded navigation cards arranged in 2 rows:
  - Row 1 (3 cards): Daily, Overview, Settings
  - Row 2 (2 cards, centred): Allocation, Calendar
- Each card: 160×160px, 12px corner radius, icon + label
- Cards highlight (`#2d2d30`) on hover

**Interactions:**
- Click any card → navigate to that page
- No toolbar buttons, no API calls

---

### 2. Daily Page

**Function:** Placeholder. Shows centred "Daily" text. Not yet implemented.

**Layout:** Solid `PANEL_BG` fill, centred "Daily" label.

**Interactions:** None. No API calls.

---

### 3. Overview Page (Gantt Chart)

**Function:** Full-window interactive Gantt chart. Primary editing hub for tasks and milestones.

**Layout:**
```
[← back][Today][+ task][◇ milestone][⌕ search]        [👤 users][⚙ settings]
[month header                                                                  ]
[day header                                                                    ]
[row 0: packed gantt bars for row 0 items                                      ]
[row 1: ...                                                                    ]
...
```

**Toolbar buttons (left-to-right):**
| # | Icon | Action |
|---|---|---|
| 0 | Today (calendar icon) | Centre view on today's date |
| 1 | Plus (+) | Open **TaskFormWindow** (new task) |
| 2 | Diamond (◇) | Open **MilestoneFormWindow** (new milestone) |
| 3 | Search (⌕) | Open **SearchWindow** |
| 4 (right) | Person (👤) | Open **UsersWindow** |
| 5 (right) | Settings (⚙) | Open **PlanSettingsWindow** |

**Gantt rendering:**
- Header: month row (18px) + day row (28px), both sticky at top
- Today is highlighted with a tinted column background (`#405bc8f5`)
- Rows are packed (tasks sharing no date overlap share a row); row height = 36px
- Task bars: rounded rectangles, coloured by status. Constraint-violation tasks show a warning icon (⚠).
- Milestones: diamond shapes
- Hover: dependency lines drawn from hovered node to all its dependencies and dependents (different colours)
- Hover tooltip panel: shows name, context label, scheduled dates, status, workers, constraint

**Context labels:** If `context_label` is set on a task/milestone, it displays as `"name | context"` in bar labels and hover panel.

**Interactions:**
- **Scroll wheel** (no modifier): scroll vertically + apply horizontal momentum
- **Scroll wheel + Shift**: zoom in/out (smooth lerp toward zoom target, pivoting at cursor)
- **Drag** in Gantt area: pan horizontally and vertically with momentum
- **Click** on task bar: open **TaskFormWindow** (edit)
- **Click** on milestone: open **MilestoneFormWindow** (edit)
- **Search result**: fly to node, flash it 3× then fade

**On-show behaviour:** Centres today's date horizontally when navigating to page.

**API calls triggered:**
- Via TaskFormWindow: `CreateTask`, `UpdateTask`, `DeleteTask`
- Via MilestoneFormWindow: `CreateMilestone`, `UpdateMilestone`, `DeleteMilestone`
- Via UsersWindow: `CreateUser`, `UpdateUser`, `DeleteUser`, `SetUserSchedule`, `ClearUserSchedule`, `SetDefaultSchedule`, `AddTag`, `RenameTag`, `DeleteTag`, `MoveTag`
- Via PlanSettingsWindow: `UpdatePlanSettings`

---

### 4. Allocation Page

**Function:** Per-user daily workload timeline. Shows which tasks each user works on each day, with utilisation colouring.

**Layout:**
```
[← back][Today]                                          [👤 users][⚙ settings]
[user panel (220px)]  [task label column (200px)]  [timeline (remaining width)]
[user 1 row          | task A                     | ■■■□□■■□□□ ... ]
[user 2 row          | task B                     | □□■■■□□■■■ ... ]
...
```

- **User panel** (left, 220px): scrollable list of users sorted by name, each row shows name + utilisation bar (green < 80%, amber 80–99%, red ≥ 100%)
- **Selected user**: clicking a user row highlights it; the task label column and timeline update to show only that user's tasks
- **Task label column** (200px): shows task names (with context label if present) for the selected user, sorted alphabetically; vertically scrollable
- **Timeline** (remaining): horizontal bars for each task's allocation segments; date header at top; vertically scrolls with task label column

**Toolbar buttons:**
| # | Icon | Action |
|---|---|---|
| 0 | Today | Centre timeline on today |
| 1 | Person (👤) | Open **UsersWindow** |
| 2 | Settings (⚙) | Open **PlanSettingsWindow** |

**Interactions:**
- **Click user row**: select user (deselects on second click); resets task scroll to 0
- **Click task label row**: open **TaskFormWindow** (edit that task)
- **Scroll** over user panel: scroll user list vertically
- **Scroll** over task label column: scroll task rows vertically
- **Scroll** over timeline (no user selected): scroll horizontally
- **Scroll + Shift**: zoom
- **Drag** in timeline: pan horizontally with momentum

**On-show behaviour:** Centres today's date in the timeline.

---

### 5. Calendar Overrides Page

**Function:** Edit per-date working-hour exceptions for the plan or individual users.

**Layout:**
```
[← back][Today ← →]                                      [⚙ settings]
[user tabs: Plan | User1 | User2 | ... | [more ▼]]
[month calendar grid                                      ]
[  Mo Tu We Th Fr Sa Su                                   ]
[  1  2  3  4  5  6  7  ← each cell shows hours if overridden, else default ]
```

**Toolbar buttons:**
| # | Action |
|---|---|
| 0 | Previous month (◄) |
| 1 | Next month (►) |
| (right) | Settings (⚙) → **PlanSettingsWindow** |

**User selector tabs:** "Plan" tab + one tab per user (sorted). Overflow users shown in a dropdown.

**Cell interaction:**
- Hover: cell highlights
- Click: opens inline edit popup with a number input (hours) and Clear/OK buttons
- OK: sends `SetCalendarOverride(date, hours)` or `SetUserCalendarOverride(userId, date, hours)`
- Clear: sends `ClearCalendarOverride(date)` or `ClearUserCalendarOverride(userId, date)`
- Enter key: confirm; Escape: cancel

**Inline edit input:** single-line text field (numeric), supports backspace, typed digits.

**On-show behaviour:** Centres on today's month.

---

### 6. Settings Page

**Function:** Plan file management and current-user identity selection. Not the same as "plan settings" (name/date).

**Layout:**
```
[← back]

Plan Management
[Save Plan]  [New Plan]

Saved Plans:
┌────────────────────────────────────────────────────────┐
│ Plan name 1                   [Load]  [Delete]          │
│ Plan name 2                   [Load]  [Delete]          │
│ ...  (scrollable, 5 rows visible)                       │
└────────────────────────────────────────────────────────┘

[Monday.com Integration]

Identity
○ (no user — plan-wide view)
○ Alice
○ Bob
...
```

**Buttons and actions:**
| Element | API call |
|---|---|
| Save Plan | `SavePlan` |
| New Plan | Opens **NewPlanWindow** |
| Load (per row) | `LoadPlan { plan_id }` |
| Delete (per row) | `DeletePlan { plan_id }` |
| Monday.com Integration | Opens **MondayWindow** |
| Identity radio | `SetCurrentUser(Option<UserId>)` |

**Plan list:** fetched via `ListPlans` (returns `PlanList(Vec<(Uuid, name, timestamp)>)`). Shown in a fixed-height scrollable box (5 rows visible). Scrolls when there are more than 5 plans.

**Page-level scroll:** The full settings page content is vertically scrollable when the window is short.

---

## Floating Windows

All floating windows render as a modal overlay (dark semi-transparent backdrop) with a centred panel. Escape closes the window (default `FloatingWindow` behaviour).

---

### TaskFormWindow

**Purpose:** Create a new task or edit an existing one.

**Opens from:** Overview toolbar (+ button), Overview Gantt click, Allocation label column click, Search result click.

**Panel size:** ~480px wide × ~840px tall (fixed, scrollable content inside)

**Sections (top-to-bottom):**

1. **Title bar:** "New Task" or task name; × close button
2. **Name** — `TextInput`, single line
3. **Description** — `MultiLineInput`, 8 visible lines
4. **Status** — segmented radio: Not Started | In Progress | Paused | Complete | Dropped
5. **Duration (days)** — `TextInput` (numeric); 0 means "derive from workload"
6. **Constraint** — radio: None | Earliest | Fixed | Latest, plus calendar date picker
7. **Actual Start / Actual End** — two calendar date pickers side-by-side
8. **Workers** — up to 3 visible rows, each row: [T/P toggle] [user picker dropdown] [workload input] [× remove]; [+ Add Worker] button; scrollable if > 3
9. **Dependencies** — up to 3 visible rows, each row: [dependency picker dropdown] [lag input] [× remove]; [+ Add Dependency] button
10. **Forward Dependents** (edit mode only) — same structure; edits the downstream nodes' dependency lists
11. **Save** (primary blue) / **Delete** (red, edit mode only)

**Worker user picker dropdown:** filter text input + scrollable list of plan users.

**Dependency picker dropdown:** filter text input + scrollable list of all tasks, milestones, and "Plan Start".

**Calendar date picker:** inline calendar popup with month nav (◄◄ ◄ ► ►►), day grid, Clear/Today shortcuts.

**API calls:**
- New: `CreateTask(Task)` on Save
- Edit: `UpdateTask(id, TaskPatch)` on Save; `DeleteTask(id)` on Delete
- When editing forward dependents: `UpdateTask` or `UpdateMilestone` on the downstream nodes

---

### MilestoneFormWindow

**Purpose:** Create or edit a milestone.

**Opens from:** Overview toolbar (◇ button), Overview Gantt click, Search result click.

**Sections:**
1. **Title bar:** "New Milestone" or milestone name
2. **Name** — `TextInput`
3. **Description** — `MultiLineInput`
4. **Constraint** — None | Earliest | Fixed | Latest + calendar picker
5. **Dependencies** — same as TaskFormWindow
6. **Forward Dependents** (edit mode only)
7. **Save** / **Delete**

**API calls:** `CreateMilestone`, `UpdateMilestone`, `DeleteMilestone`, plus `UpdateTask`/`UpdateMilestone` for forward dependent edits.

---

### UsersWindow

**Purpose:** Manage plan users, their tags, and their work schedules.

**Opens from:** Overview and Allocation toolbar (👤 button).

**Layout:**
- List of users (sorted by name)
- Each row: user name | [Edit] | [Schedule] | [Delete]
- [+ Add User] button at top
- [Manage Tags] button

**Actions:**
| Action | Opens | API call |
|---|---|---|
| Add User | **UserFormWindow** (new) | — |
| Edit row | **UserFormWindow** (edit) | — |
| Schedule row | **ScheduleWindow** (for that user) | — |
| Delete row | Immediate | `DeleteUser(UserId)` |
| Manage Tags | **TagsWindow** | — |

---

### UserFormWindow

**Purpose:** Create or edit a user's name and skill/role tags.

**Sections:**
1. **Name** — `TextInput`
2. **Tags** — multi-select list of plan tags (checkboxes)
3. **Save** / **Delete** (edit mode)

**API calls:** `CreateUser(User)` or `UpdateUser(UserId, UserPatch)`, `DeleteUser(UserId)`.

---

### ScheduleWindow

**Purpose:** Edit working hours per weekday for the plan default schedule or a specific user.

**Opens from:** UsersWindow (per-user), or accessible via the plan settings area.

**Layout:**
- Title: "Default Schedule" or "{User} Schedule"
- 7 rows (Mon–Sun): day label + number input (hours)
- [Save] button
- [Reset to Default] button (user schedule only)

**API calls:**
- Plan: `SetDefaultSchedule(WorkSchedule)`
- User: `SetUserSchedule(UserId, WorkSchedule)` or `ClearUserSchedule(UserId)` (Reset)

---

### TagsWindow

**Purpose:** Manage the ordered list of skill/role tags.

**Opens from:** UsersWindow → "Manage Tags".

**Layout:**
- List of tags with drag-handles (reorder) + rename inline + [Delete] button
- [+ Add Tag] input at bottom

**API calls:** `AddTag(name)`, `RenameTag(id, name)`, `DeleteTag(id)`, `MoveTag(id, newIndex)`.

---

### SearchWindow

**Purpose:** Search tasks and milestones by name. Selecting a result closes the window and scrolls the Gantt to that node (with a 3-pulse flash animation).

**Opens from:** Overview toolbar (⌕ button).

**Layout:**
- Filter text input (auto-focused)
- Scrollable results list (tasks + milestones matching filter)
- Each row: icon + name (+ context label if present)

**Interactions:**
- Type to filter; click result or press Enter to navigate
- Result is written into a shared `Arc<Mutex<Option<NodeId>>>` channel; Overview's `tick_animation` drains it

**API calls:** None (read-only).

---

### PlanSettingsWindow

**Purpose:** Edit the current plan's display name, start date, and the scheduler target node.

**Opens from:** Overview toolbar (⚙), Allocation toolbar (⚙), Calendar Overrides toolbar (⚙).

**Fields:**
1. **Plan Name** — `TextInput`
2. **Start Date** — calendar date picker
3. **Scheduler Target** — dropdown: "Plan Start" + all task and milestone names

**API calls:** `UpdatePlanSettings { name, start_date, scheduler_target }` on Save.
Note: saving triggers a scheduler recompute on the server.

---

### NewPlanWindow

**Purpose:** Create a new blank plan with a given name and start date.

**Fields:**
1. **Plan Name** — `TextInput`
2. **Start Date** — calendar date picker

**API calls (in order):**
1. `NewPlan` — creates blank plan and switches to it
2. `UpdatePlanSettings { name, start_date, scheduler_target: PlanStart }`

---

### MondayWindow

**Purpose:** Configure and run Monday.com board synchronisation for the current plan. Configuration is stored to disk (`plans/<uuid>/monday.json`); the API token is stored in `config.json`.

**Opens from:** Settings page → "Monday.com Integration" button.

**Panel:** 480px wide, scrollable content. Sections:

#### Connection
- **API Token** — `TextInput` (value stored globally, not per-plan)
- **Board ID** — `TextInput`
- **[Test Connection]** — fires HTTP `me { name }` query; shows success/error status

#### Column Mapping
- **Person Column ID** — `TextInput`
- **Status Column ID** — `TextInput`
- **Dependency Column ID** — `TextInput`
- **Workload Column ID** — `TextInput`
- **Timeline Column ID** — `TextInput` (milestones auto-detected via this column)
- **[Fetch Board Info]** — background thread: fetches columns, users, status labels from API; auto-populates dropdowns

#### Item Type
- Radio: **Subitems** | **Items** — whether to import subitems or top-level board items

#### Workload Unit
- Radio: **Hours** | **Days**

#### Show Group/Parent Context
- Radio: **On** | **Off** — if On, `context_label` is populated from group name (items) or parent item name (subitems) on next pull

#### User Mappings
- List of Monday workspace users (fetched via Fetch Board Info)
- Each row: Monday name → plinko user picker dropdown (or "Unassigned")

#### Status Mappings
- List of Monday status labels (fetched)
- Each row: Monday label → plinko Status picker

#### Sync Actions
- **[Pull from Monday]** — background thread: calls `import_from_monday()`, then sends `PlanRequest::ReplacePlan` and saves updated `MondayConfig`
- **[Full Re-import]** — same but clears existing node mappings first (re-creates all tasks/milestones from scratch)
- **[Push dates to Monday]** — background thread: diff-based export; updates status, timeline, person, workload, dependencies on Monday; creates new items for untracked plinko nodes
- Status message shown below action buttons (with progress counter during push)

**Notes on background threading (current implementation, to be changed in migration):**
- All HTTP calls run in `std::thread::spawn` threads
- Results communicated back via `Arc<Mutex<...>>` channels
- `tick_animation` polls these channels each frame to update status and consume results
- Monday logic (HTTP client, import, export) lives entirely in `plinko-ui/src/monday/`

---

### ErrorWindow

**Purpose:** Display an error message. Shown when the server returns a `PlanError` or a background operation fails.

**Layout:** Title "Error" + message text + [OK] button.

---

## Shared UI Widgets

### TextInput (`plinko-ui/src/ui/text_input.rs`)
- Single-line editable text field
- Features: cursor, text selection, horizontal scroll (via `Cell<f32>` for render-time mutation), clipboard paste
- Key handling via `handle_key(key, modifiers) -> bool` and `handle_paste(text)`
- Ctrl+←/→: word jump; Home/End: line boundaries; Backspace/Delete

### MultiLineInput (`plinko-ui/src/ui/multi_line_input.rs`)
- Multi-line editable text area with vertical scroll
- Key handling via `handle_key(key, modifiers, inner_width, line_h, visible_h, font)` and `handle_paste(...)`

### CalendarPicker (embedded in TaskFormWindow, MilestoneFormWindow, PlanSettingsWindow, NewPlanWindow)
- Month/year navigation (◄◄ ◄ ► ►►)
- Day grid (Mon–Sun columns)
- Clear / Today shortcut buttons
- Value displayed as a trigger button ("15 Apr 2025") that opens the popup

---

## Color Palette (key values)

| Token | Value | Usage |
|---|---|---|
| `PANEL_BG` | `#252526` | Window/card backgrounds |
| `GANTT_BG` | `#1e1e1e` | Gantt/timeline background |
| `GANTT_HEADER_BG` | `#252526` | Header rows |
| `BTN_PRIMARY_BG` | `#4a90d9` | Save / confirm buttons |
| `BTN_DANGER_BG` | `#e53935` | Delete buttons |
| `BTN_SECONDARY_BG` | `#2d2d30` | Secondary/cancel buttons |
| `GANTT_TODAY_BG` | `#405bc8f5` | Today column tint |
| `INPUT_BG` | (dark) | Text field backgrounds |
| `INPUT_BORDER_FOCUS` | (blue) | Focused input border |
| `HOME_CARD_BG` | `#252526` | Home nav cards |
| `HOME_CARD_HOVER_BG` | `#2d2d30` | Hovered cards |

---

## Scroll/Zoom Conventions

- **Shift + scroll** = zoom (all calendar pages)
- **Drag** in timeline area = pan with momentum
- **Vertical scroll** over user panel = scroll user list
- **Vertical scroll** over task label column (allocation) = scroll task rows
- **Vertical scroll** in timeline when user selected (allocation) = scroll task rows
- **On-show**: all calendar pages (`on_show` callback) centre today's date

---

## Monday Integration (Current Architecture — UI-side)

All Monday logic currently lives in `plinko-ui/src/monday/`:

| File | Responsibility |
|---|---|
| `client.rs` | HTTP client (`MondayClient`); all GraphQL mutations and queries |
| `import.rs` | `import_from_monday()` — pulls items, builds Plan in memory, sends `ReplacePlan` |
| `export.rs` | `export_to_monday_diff()` — computes diff, sends mutations per op |
| `mod.rs` | Re-exports |

`MondayConfig` (per-plan, persisted) and `MondayItem` (ephemeral, used during import) live in `plinko-shared/src/monday/`.

The `MondayWindow` manages all user interaction and spawns background threads for:
- Fetch Board Info (`pending_board_result: Arc<Mutex<...>>`)
- Pull from Monday (`sync_state: Arc<Mutex<SyncState>>`)
- Push to Monday (`push_progress: Arc<Mutex<Option<(usize, usize)>>>`, `push_status_msg: Arc<Mutex<String>>`)

---

## Storage

- Plans stored under `$XDG_DATA_HOME/plinko/plans/<uuid>/` as timestamped JSON snapshots
- Monday config: `plans/<uuid>/monday.json`
- Global API token: `config.json`
- `Storage::from_user_data_dir()` in production; `Storage::from_path(tmp)` in tests
