# Plinko Project - Complete Exploration

## Project Structure

**src/** directory contains:
```
src/
├── app.rs                 # Top-level Application struct
├── engine.rs              # Plan request engine & dispatch
├── main.rs                # Entry point
├── data/                  # Domain models
│   ├── mod.rs
│   ├── plan.rs            # Plan aggregate root
│   ├── calendar.rs        # CalendarOverrides
│   ├── schedule.rs        # WorkSchedule & Weekday
│   ├── ids.rs
│   ├── task.rs
│   ├── user.rs
│   ├── milestone.rs
│   ├── dependency.rs
│   ├── constraint.rs
│   ├── allocation.rs
│   ├── scheduler.rs
│   └── ...
├── pages/                 # Page system
│   ├── mod.rs             # Page trait & PageManager
│   ├── home/              # Home page (card grid)
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   └── render.rs
│   ├── daily/             # Daily page (stub)
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   └── render.rs
│   ├── overview/          # Overview/Gantt page
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── render.rs
│   │   ├── gantt.rs
│   │   └── ...
│   └── settings/          # Settings page (stub)
├── ui/                    # UI components & windows
│   ├── cache.rs           # RenderCache
│   ├── layout.rs          # Constants & colors
│   ├── icons.rs           # Icon builders
│   ├── toolbar.rs         # [NOT FOUND - doesn't exist]
│   ├── plan_settings_window.rs
│   ├── back_button.rs
│   ├── icon_button.rs
│   ├── floating_window.rs
│   ├── task_form_window.rs
│   ├── milestone_form_window.rs
│   ├── users_window.rs
│   ├── schedule_window.rs
│   └── ...
└── graphics/              # OpenGL/Skia environment
    └── env.rs
```

---

## 1. engine.rs (1022 lines) - Request Queue & Dispatch

### Key Enums

**PlanRequest enum** - All mutations flow through this:
- **Scheduling**: `RunScheduler`
- **Task Lifecycle** (not validated): `StartTask`, `PauseTask`, `ResumeTask`, `CompleteTask`, `DropTask`
- **Task CRUD** (validated): `CreateTask`, `UpdateTask(TaskId, TaskPatch)`, `DeleteTask`
- **Milestone CRUD**: `CreateMilestone`, `UpdateMilestone`, `DeleteMilestone`
- **User CRUD**: `CreateUser`, `UpdateUser(UserId, UserPatch)`, `DeleteUser`
- **User Schedules**: `SetUserSchedule(UserId, WorkSchedule)`, `ClearUserSchedule`
- **Plan-wide Schedule**: `SetDefaultSchedule(WorkSchedule)` ← **SPECIAL HANDLER**
- **Tag Operations**: `AddTag`, `RenameTag`, `DeleteTag`, `MoveTag`
- **Plan Metadata**: `UpdatePlanSettings { name, start_date, scheduler_target }`

**PlanResponse enum**:
- `PlanUpdated` - mutation succeeded
- `Error(PlanError)` - failed

**PlanError enum**:
- `TaskNotFound(TaskId)`
- `MilestoneNotFound(MilestoneId)`
- `UserNotFound(UserId)`
- `Scheduler(SchedulerError)`
- `Dependency(DependencyError)`

### Patch Types

**TaskPatch** (all fields Option-wrapped):
```rust
pub struct TaskPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub actual_start_date: Option<Option<NaiveDate>>,  // Option<None> clears
    pub actual_end_date: Option<Option<NaiveDate>>,
    pub constraint: Option<Option<DateConstraint>>,
    pub duration_days_target: Option<f32>,
    pub workers: Option<Vec<WorkerSlot>>,
    pub dependencies: Option<Vec<Dependency>>,
}
```
Uses chainable setters: `.name("x").duration_days_target(5.0)`

**MilestonePatch** - Similar pattern with name, description, constraint, dependencies

**UserPatch**:
```rust
pub struct UserPatch {
    pub name: Option<String>,
    pub tags: Option<HashSet<TagId>>,
    pub avatar: Option<Option<Vec<u8>>>,
}
```

### PlanRequestSender

- Clonable handle for submitting requests
- `send(&self, request: PlanRequest)` - fires async
- Channel-based, drops silently if engine shut down

### PlanEngine

**Key methods**:
- `new(plan: Plan) -> Self`
- `sender(&self) -> PlanRequestSender`
- `plan(&self) -> &Plan` - read-only access
- `process_pending() -> Vec<PlanResponse>` - drain queue, process each, re-run scheduler

**Validation System**:
- `apply_validated<F>(&mut self, f: F) -> PlanResponse` where F mutates plan
- Backs up plan if allocation exists
- Runs scheduler after mutation
- Restores backup on scheduler failure (if had prior schedule)
- If no prior schedule, keeps mutation but logs warning

**Special Handler: SetDefaultSchedule**
```rust
PlanRequest::SetDefaultSchedule(schedule) => self.apply_validated(|plan| {
    plan.default_schedule = schedule;
    Ok(())
})
```
- Validated: backs up, runs scheduler
- Affects all users without overrides
- Can break existing schedule

---

## 2. pages/mod.rs - Page System

### PageId enum
```rust
pub enum PageId {
    Home,
    Daily,
    Overview,
    Settings,
}
```

### Page trait
```rust
pub trait Page {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan);
    fn on_cursor_moved(&mut self, x: f32, y: f32, width: f32, height: f32, plan: &Plan) -> DirtyRegion;
    fn on_mouse_input(&mut self, x: f32, y: f32, pressed: bool, width: f32, height: f32, 
                      plan: &Plan, sender: &PlanRequestSender) -> DirtyRegion;
    fn on_key_input(&mut self, key: &winit::keyboard::Key, sender: &PlanRequestSender) -> DirtyRegion;
    fn on_scroll(&mut self, delta_y: f32, shift: bool, width: f32, height: f32, plan: &Plan) -> DirtyRegion;
    fn reset_hover(&mut self) {}
    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> { None }
    fn tick_animation(&mut self, width: f32, height: f32, plan: &Plan) -> DirtyRegion { DirtyRegion::None }
    fn has_animation(&self) -> bool { false }
}
```

### PageManager struct
```rust
pub struct PageManager {
    pub active: PageId,
    pub home: home::HomePage,
    pub daily: daily::DailyPage,
    pub overview: overview::OverviewPage,
    pub settings: settings::SettingsPage,
}
```

**Key methods**:
- `active_page(&self) -> &dyn Page` - shared reference to active page
- `active_page_mut(&mut self) -> &mut dyn Page` - mutable reference
- `set_active(&mut self, page: PageId)` - switch active page

---

## 3. pages/overview/mod.rs - Gantt Chart Page

### OverviewPage struct
```rust
pub struct OverviewPage {
    pub state: OverviewState,
}
```

### Page trait implementation

**render()** → calls `render::draw_overview()`

**on_cursor_moved()**:
- Tracks cursor position
- Hit-tests toolbar buttons for hover
- If dragging: updates scroll positions, applies momentum damping
- Returns `DirtyRegion::PageOnly` if toolbar hover changed

**on_mouse_input()**:
- **Pressed above toolbar**: Start dragging (track velocity)
- **Pressed in toolbar area**: Handle button clicks
  - Button 0 (Today): Center Gantt on today's date
  - Button 1 (Plus): Set flag `open_task_form = true`
  - Button 2 (Diamond): Set flag `open_milestone_form = true`
  - Button 3 (Person): Set flag `open_users_window = true`
  - Button 4 (Settings): Capture plan state, set flag `open_settings_window = true`
- **Released while dragging**: Check drag distance, compute momentum
  - If < 6px: treat as click, hit-test Gantt items
  - If > 6px: apply inertia to velocities

**on_scroll()** (momentum scrolling):
- **Shift+scroll**: Nudge `zoom_target`, `tick_animation` lerps smoothly
- **Regular scroll**: Apply velocity with damping to vertical scroll

**take_open_request()**:
- Consumes `pending_window` (from Gantt clicks)
- Consumes flags (`open_task_form`, `open_milestone_form`, etc.)
- Returns floating window or `None`

**tick_animation()**:
- Smooth zoom interpolation around cursor pivot
- Velocity-based inertial scrolling (friction = 0.88)
- Clamps scroll_y to valid range
- Returns `DirtyRegion::PageOnly` if anything changed

**has_animation()**: True if vel_x/vel_y/zoom_target differ significantly

---

## 4. pages/overview/state.rs - Gantt State

### OverviewState struct
```rust
pub struct OverviewState {
    // Toolbar
    pub toolbar_btn_hovered: Option<usize>,
    pub open_users_window: bool,
    pub open_task_form: bool,
    pub open_milestone_form: bool,
    pub pending_window: Option<Box<dyn FloatingWindow>>,
    
    // Gantt scrolling
    pub scroll_y: f32,
    pub zoom: f32,
    pub cursor_x: f32,
    pub cursor_y: f32,
    
    // Momentum
    pub vel_x: f32,
    pub vel_y: f32,
    pub zoom_vel: f32,
    pub zoom_target: f32,
    
    // Dragging
    pub is_dragging: bool,
    pub last_drag_x: f32,
    pub last_drag_y: f32,
    pub drag_vel_x: f32,
    pub drag_vel_y: f32,
    pub press_start_x: f32,
    pub press_start_y: f32,
    pub scroll_x: f32,
    
    // Settings window
    pub open_settings_window: bool,
    pub settings_init_name: String,
    pub settings_init_date: String,
    pub settings_init_scheduler_target: NodeId,
}
```

---

## 5. pages/overview/render.rs (929 lines) - Gantt Rendering

### draw_gantt_header()
Renders month and day labels in two-row header:
- **Month row**: "Jan 2026", "Feb 2026", etc., centred in visible segment
- **Day row** (if zoom >= 16): Day numbers (1-31)
- Day segments computed smoothly from actual month boundaries
- Bottom border line separates from grid

**Key layout calculations**:
```rust
fn date_to_x(date: NaiveDate, view_start: NaiveDate, zoom: f32, scroll_x: f32) -> f32 {
    let days = (date - view_start).num_days();
    days as f32 * zoom - scroll_x
}

fn gantt_header_top() -> f32 {
    TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE + 8.0
}

fn gantt_rows_top() -> f32 {
    gantt_header_top() + GANTT_HEADER_H
}

fn vertical_center_offset(num_rows: usize, height: f32) -> f32 {
    let content_h = num_rows as f32 * GANTT_ROW_H;
    let visible_h = (height - gantt_rows_top()).max(0.0);
    ((visible_h - content_h) / 2.0).max(0.0)
}

fn row_top_y(row_idx: usize, num_rows: usize, height: f32, scroll_y: f32) -> f32 {
    gantt_rows_top() + vertical_center_offset(num_rows, height) 
        + row_idx as f32 * GANTT_ROW_H - scroll_y
}
```

### Rendering layers (in order)
1. `draw_gantt_row_backgrounds()` - Alternating row stripes
2. `draw_gantt_grid()` - Vertical day separators, weekend shading
3. `draw_gantt_rows()` - Task bars & milestone diamonds
4. `draw_gantt_dependencies()` - Arrow connectors
5. `draw_gantt_header()` - Month/day labels
6. `draw_toolbar_buttons()` - Icon buttons

### Task bars & milestone diamonds

**Tasks**:
- Bar position: `date_to_x(start_date, ...) to date_to_x(end_date, ...)`
- Height: `GANTT_ROW_H - 2 * GANTT_ROW_PADDING`
- Color: mapped from `TaskStatus` (NotStarted, InProgress, OnHold, Complete, Dropped)
- Label: Centred, auto-coloured (light text on dark, dark on light)

**Milestones**:
- Diamond (rotated square) at midpoint of date
- Size: `GANTT_MS_HALF` (10px half-width by default)
- Color: mapped from status
- Label: Placed right/left/bottom/top (whichever is clear)

**Plan Start marker**:
- Special diamond, slightly larger, teal colour
- Label: "Plan Start"

### Dependency arrows

Drawn between end of source item and start of destination item:
- **Same row**: Horizontal line → right-pointing arrowhead
- **Different rows**: S-curve path with vertical arrowhead
  - Tries to route through midpoint if destination is right of source
  - Otherwise drops straight down/up from source

### Hit testing

**hit_test_toolbar_buttons(px, py, width)**:
- Tests 5 icon buttons (today, plus, diamond, person, settings)
- Returns button index (0-4) or None

**hit_test_gantt_item(x, y, rows, state, height, view_start)**:
- Tests all task bars and milestone diamonds
- Tasks: rectangular hit test
- Milestones: Manhattan distance ≤ 1.5 * GANTT_MS_HALF
- Returns `GanttHit::Task(id)` or `GanttHit::Milestone(id)` or None

---

## 6. pages/home/mod.rs & state.rs & render.rs

### HomePage struct
```rust
pub struct HomePage {
    pub state: state::HomeState,
}
```

### HomeState
```rust
pub struct HomeState {
    pub hovered_card: Option<usize>,  // 0=Daily, 1=Overview, 2=Settings
}
```

### Rendering (render.rs)

**card_rects(width, height) -> [Rect; 3]**:
- Three cards centred horizontally and vertically
- Size: `HOME_CARD_SIZE` (160px)
- Gap: `HOME_CARD_GAP` (32px)
- Corner radius: `HOME_CARD_CORNER` (12px)

**draw_home(canvas, width, height, hovered_card, cache)**:
- Background fill (light gray)
- For each card:
  - Background (white, or hover color)
  - Border
  - Icon (stroke width 1.5px) centred in upper portion
  - Label (centred at 78% height)

**hit_test_card(x, y, width, height) -> Option<usize>**:
- Simple AABB test against three card rects

### Page trait implementation

**on_cursor_moved()**: Update `hovered_card`, return `DirtyRegion::PageOnly` if changed

**on_mouse_input()**: Return `None` (navigation handled in app.rs)

---

## 7. ui/cache.rs - RenderCache

### RenderCache struct
```rust
pub struct RenderCache {
    pub font: Font,                       // 16pt sans-serif
    pub small_font: Font,                 // 12pt sans-serif
    pub home_icon_paths: [Path; 3],       // daily, planning, settings
    pub home_card_labels: [TextBlob; 3],  // "Daily", "Overview", "Settings"
    pub icon_person: Path,
    pub icon_plus: Path,
    pub icon_diamond: Path,
    pub icon_tag: Path,
    pub icon_settings: Path,
    pub icon_today: Path,
    pub daily_label: TextBlob,
    pub left_panel_label: TextBlob,
    pub right_panel_label: TextBlob,
    pub settings_label: TextBlob,
}
```

### new() method
- Resolves sans-serif font via `FontMgr::match_family_style()`
- Builds all icon paths (see ui/icons.rs)
- Pre-renders all text blobs
- Returns fully initialized cache

All resources are built once at startup and reused every frame.

---

## 8. ui/layout.rs - Constants & Colors

### Layout Constants

**General**:
- `DIVIDER_WIDTH: 6.0`

**Home page**:
- `HOME_CARD_SIZE: 160.0`
- `HOME_CARD_GAP: 32.0`
- `HOME_CARD_CORNER: 12.0`
- `HOME_CARD_ICON_SIZE: 48.0`

**Back button** (top-left):
- `BACK_BTN_X: 16.0`, `BACK_BTN_Y: 16.0`
- `BACK_BTN_SIZE: 36.0`
- `BACK_BTN_CORNER: 4.0`

**Toolbar buttons** (page-specific, top-left):
- `TOOLBAR_BTN_GAP: 8.0`
- `TOOLBAR_BTN_Y: 16.0` (same as back button)
- `TOOLBAR_BTN_SIZE: 36.0`
- `toolbar_btn_x(n: u32) -> f32` = `16 + 36 + 8 + n*(36+8)`
- `settings_btn_x(window_width)` → right side
- `person_right_btn_x(window_width)` → left of settings

**Gantt chart**:
- `GANTT_MONTH_ROW_H: 18.0`
- `GANTT_DAY_ROW_H: 28.0`
- `GANTT_HEADER_H: 46.0` (month + day rows)
- `GANTT_ROW_H: 36.0` (height of each task/milestone row)
- `GANTT_ROW_PADDING: 5.0`
- `GANTT_BAR_CORNER: 4.0`
- `GANTT_DAY_LINE_W: 6.0`
- `GANTT_ZOOM_DEFAULT: 40.0` (pixels per day)
- `GANTT_ZOOM_MIN: 8.0`, `GANTT_ZOOM_MAX: 200.0`
- `GANTT_MS_HALF: 10.0` (milestone diamond half-size)

### Color Palette (as 0xAA_RRGGBB)

**Gantt task status colors**:
- `GANTT_TASK_NOT_STARTED: 0xff_d0d0d0` (light gray)
- `GANTT_TASK_IN_PROGRESS: 0xff_f5a623` (orange)
- `GANTT_TASK_ON_HOLD: 0xff_b39ddb` (lavender)
- `GANTT_TASK_COMPLETE: 0xff_66bb6a` (green)
- `GANTT_TASK_DROPPED: 0xff_757575` (dark gray)

**Gantt milestone status colors**:
- `GANTT_MS_NOT_STARTED: 0xff_bdbdbd`
- `GANTT_MS_IN_PROGRESS: 0xff_f5a623`
- `GANTT_MS_COMPLETE: 0xff_66bb6a`
- `GANTT_PLAN_START_COLOR: 0xff_00897b` (teal)

**Gantt UI colors**:
- `GANTT_BG: 0xff_fafafa`
- `GANTT_HEADER_BG: 0xff_f0f0f0`
- `GANTT_HEADER_BORDER: 0xff_d8d8d8`
- `GANTT_ROW_ALT_BG: 0xff_f4f4f4` (alternating rows)
- `GANTT_WEEKEND_BG: 0xff_efefef`
- `GANTT_TODAY_LINE_COLOR: 0x80_4a90d9` (semi-transparent blue)
- `GANTT_DEP_LINE_COLOR: 0x80_888888` (semi-transparent gray)

**Home page colors**:
- `HOME_BG: 0xff_f5f5f5`
- `HOME_CARD_BG: 0xff_ffffff`
- `HOME_CARD_HOVER_BG: 0xff_e8e8e8`
- `HOME_CARD_BORDER: 0xff_e0e0e0`

---

## 9. ui/icons.rs - Icon Builders

All return `Path` objects drawn in `w × h` bounding box starting at origin (caller translates).

### Icon functions
- **`build_icon_daily(w, h)`**: Calendar outline + header bar + hangers + dot
- **`build_icon_planning(w, h)`**: Two-column split-view rectangles
- **`build_icon_settings(w, h)`**: Three horizontal slider lines with circular knobs
- **`build_icon_plus(w, h)`**: Vertical & horizontal bars crossing at centre
- **`build_icon_diamond(w, h)`**: Rotated square (milestone marker)
- **`build_icon_person(w, h)`**: Head circle + shoulders arc
- **`build_icon_tag(w, h)`**: Gift-tag shape (pointed left, rounded right) + hole, rotated 135°
- **`build_icon_today(w, h)`**: Vertical line + left-pointing arrowhead at mid height

---

## 10. app.rs (595 lines) - Application Handler

### Application struct
```rust
pub struct Application {
    pub env: Env,
    pub fb_info: FramebufferInfo,
    pub num_samples: usize,
    pub stencil_size: usize,
    modifiers: Modifiers,
    scale_factor: f64,
    cache: RenderCache,
    pending_dirty: DirtyRegion,
    pages: PageManager,
    cursor_pos: (f32, f32),
    app_state: AppState,
    back_hovered: bool,
    engine: PlanEngine,
    retained_surface: Option<Surface>,
    retained_size: (i32, i32),
    home_picture: Option<Picture>,
    back_picture: Option<Picture>,
    floats: FloatingWindowManager,
}

enum AppState {
    Home,
    InPage(PageId),
}
```

### Key methods

**navigate_to(page: PageId)**:
- Reset hover on home and entering page
- Switch app_state to `InPage(page)`
- Set active page
- Invalidate cached pictures
- Mark full dirty

**navigate_home()**:
- Reset hover on current page
- Switch app_state to `Home`
- Clear floating windows
- Invalidate cached pictures
- Mark full dirty

### Event handling (window_event)

**Home card clicks** (home_render::hit_test_card):
```rust
idx 0 → navigate_to(PageId::Daily)
idx 1 → navigate_to(PageId::Overview)
idx 2 → navigate_to(PageId::Settings)
```

**In-page back button** (back_button::hit_test_back_button):
→ navigate_home()

**In-page toolbar buttons** + Gantt interactions:
- Page receives on_mouse_input
- If window returned from take_open_request: floats.push(window)

### Rendering strategy

Uses retained off-screen surface for partial redraws:
- **DirtyRegion::All**: Redraw entire page
- **DirtyRegion::PageOnly**: Redraw page, keep back button
- **DirtyRegion::BackButtonOnly**: Redraw only back button region
- Picture caching for home screen and back button

### Animation loop

- Calls `active_page_mut().has_animation()`
- If true: `tick_animation()` every frame, set `ControlFlow::Poll`
- Otherwise: `ControlFlow::Wait` (power saving)

---

## 11. data/plan.rs (1030 lines) - Plan Aggregate Root

### Plan struct
```rust
pub struct Plan {
    pub id: Uuid,
    pub name: String,
    pub tasks: HashMap<TaskId, Task>,
    pub milestones: HashMap<MilestoneId, Milestone>,
    pub users: HashMap<UserId, User>,
    pub default_schedule: WorkSchedule,
    pub user_schedules: HashMap<UserId, WorkSchedule>,
    pub calendar: CalendarOverrides,
    pub user_calendars: HashMap<UserId, CalendarOverrides>,
    pub start_date: NaiveDate,
    pub dates: StartDates,
    pub scheduler_target: NodeId,
    pub allocation: Option<PlanAllocation>,
    pub tags: Vec<Tag>,
}
```

### Key methods

**schedule_for(&self, user_id: &UserId) -> &WorkSchedule**:
- Returns user's override or default

**set_user_schedule(&mut self, user_id: UserId, schedule: WorkSchedule)**:
- Sets override, invalidates allocation

**hours_available(&self, user_id: &UserId, date: NaiveDate) -> f32**:
- Resolution: user calendar → plan calendar → weekday schedule
- Falls back through hierarchy

**add_task_dependency(task_id, dep) -> Result<(), DependencyError>**:
- Validates no cycle
- Updates lag if dependency exists
- Invalidates allocation

**has_dependency_path(start, target) -> bool**:
- Transitive reachability check (public for form validation)

**Tag management**:
- `add_tag(name)` → Some(TagId) or None (duplicate)
- `rename_tag(id, new_name)` → bool
- `remove_tag(id)` - removes from users & tasks
- `move_tag(id, new_index)` - reorders registry

---

## 12. data/calendar.rs - CalendarOverrides

### CalendarOverrides struct
```rust
pub struct CalendarOverrides {
    pub entries: HashMap<NaiveDate, f32>,  // date → hours
}
```

**Methods**:
- `new()` - empty
- `set(date, hours)` - insert/update
- `remove(date)` - delete
- `get(date) -> Option<f32>` - lookup

Value of 0.0 means day is completely off; dates absent not overridden.

---

## 13. data/schedule.rs - WorkSchedule

### Weekday enum
```rust
pub enum Weekday {
    Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday,
}
```

### chrono_to_weekday() function
Converts `chrono::Weekday` to project's `Weekday`

### WorkSchedule struct
```rust
pub struct WorkSchedule {
    pub days: HashMap<Weekday, f32>,  // day → hours/day
}
```

**Methods**:
- `weekdays()` - Mon-Fri 8h each
- `full_week()` - 7 days 8h each
- `with_day(day, hours)` - chainable add/update
- `without_day(day)` - chainable remove
- `is_working_day(day) -> bool`
- `hours_on(day) -> f32`
- `total_hours_per_week() -> f32`
- `working_days_per_week() -> f32`
- `hours_per_workload_day() -> f32` - mode (highest on tie)

---

## 14. data/mod.rs - Module Re-exports

```rust
pub use allocation::{MilestoneAllocation, PlanAllocation, SlotAllocation, TaskAllocation, WorkSegment};
pub use calendar::CalendarOverrides;
pub use constraint::{ConstraintKind, DateConstraint};
pub use dates::StartDates;
pub use dependency::Dependency;
pub use ids::{MilestoneId, NodeId, TagId, TaskId, UserId};
pub use milestone::Milestone;
pub use plan::{DependencyError, Plan, Tag};
pub use schedule::{Weekday, WorkSchedule};
pub use storage::{Storage, StorageError};
pub use task::{Task, TaskStatus, WorkerSlot};
pub use user::User;
```

---

## 15. ui/plan_settings_window.rs (1695 lines) - Plan Settings Modal

### Layout Constants

**Panel**:
- Width: 520px, Height: 480px
- Corner: 8px
- Title bar: 48px

**Calendar popup**:
- Cell: 32px
- Header: 28px (month navigation)
- Day-of-week row: 20px
- Content rows: 6 × 26px
- Footer: 28px (Today, Clear buttons)
- Total H: 224px

**Target dropdown**:
- Filter row: 32px
- Item rows: 28px each (max 5 visible)
- Total H: ~172px

### CalendarPicker struct

Manages:
- Selected date value
- Navigation year/month
- All hover states (prev/next month/year, clear, today, trigger)

**Methods**:
- `prev_month()`, `next_month()`, `prev_year()`, `next_year()`
- `reset_hover()` - clear all hovers
- `display_text()` → "DD Mon YYYY" or "—"

### Helper functions

**Calendar geometry**:
- `cal_prev_year_btn(cal: Rect) -> Rect`
- `cal_prev_month_btn(cal: Rect) -> Rect`
- `cal_next_month_btn(cal: Rect) -> Rect`
- `cal_next_year_btn(cal: Rect) -> Rect`
- `cal_clear_btn(cal: Rect) -> Rect`
- `cal_today_btn(cal: Rect) -> Rect`
- `cal_day_cell(cal: Rect, day_1_offset: u32, day: u32) -> Rect`
- `calendar_popup_rect(trigger_screen: Rect, panel: Rect) -> Rect`
  - Positions popup below trigger if fits, else above
  - Keeps popup within panel bounds horizontally

**Drawing functions**:
- `draw_text_input()` - input box with border & cursor
- `draw_date_btn()` - button showing selected date + tiny calendar icon
- `draw_calendar_popup()` - full calendar with month navigation & footer buttons

---

## 16. pages/settings/mod.rs - Settings Page Stub

### SettingsPage struct
```rust
pub struct SettingsPage {
    pub state: state::SettingsState,
}
```

### Page trait impl
- `render()`: calls `render::draw_settings()` (just centred label)
- `on_cursor_moved()`: returns `None`
- `on_mouse_input()`: returns `None`

---

## 17. pages/daily/mod.rs - Daily Page Stub

### DailyPage struct
```rust
pub struct DailyPage {
    pub state: state::DailyState,
}
```

### Page trait impl
- `render()`: calls `render::draw_daily()` (just centred label)
- `on_cursor_moved()`: returns `None`
- `on_mouse_input()`: returns `None`

---

## Architecture Patterns

### Separation of Concerns
1. **Domain models** (data/) - pure business logic, no UI awareness
2. **Engine** (engine.rs) - request queue, validation, scheduling
3. **Pages** (pages/) - full-screen views, state machines
4. **UI components** (ui/) - reusable floating windows & primitives
5. **Rendering** - stateless Skia drawing functions

### Request-Response Pattern
- UI components create `PlanRequest` via sender
- Engine processes in queue, validates with backup/restore
- Returns `PlanResponse` (success or error)
- App marks dirty regions & re-renders

### State Management
- Pages hold mutable state (scroll, hover, etc.)
- Pages implement Page trait (input + rendering)
- PageManager dispatches to active page
- DirtyRegion tracks what needs redrawing

### Rendering Optimization
- Skia Picture caching for expensive operations
- Retained off-screen surface for partial redraws
- DirtyRegion: All, PageOnly, BackButtonOnly, None
- Animation loop only runs if has_animation() true

### Geometry Helpers
- Consistent coordinate transforms (physical → logical)
- Layout constants centralized in layout.rs
- Hit-testing functions in each page/window
- Clipping regions for partial rendering

---

## File Sizes & Complexity

- engine.rs: 1022 lines (patch types + request dispatch + engine)
- plan.rs: 1030 lines (aggregate root + tests)
- plan_settings_window.rs: 1695 lines (modal overlay)
- render.rs (overview): 929 lines (Gantt rendering + dependencies)
- app.rs: 595 lines (window handler + routing)
- cache.rs: 99 lines (pre-built resources)
- layout.rs: 176 lines (constants + colors)
- icons.rs: 191 lines (icon path builders)

All other files are smaller, specialized modules.

---

## Notes for Implementation

### Key Concepts
1. **Validation always backs up plan** if allocation exists, restores on scheduler failure
2. **SetDefaultSchedule is validated** - can invalidate existing schedule
3. **Gantt smoothly pans** with momentum (velocity-based inertia)
4. **Zoom pivots around cursor** - keeps same date stationary
5. **Milestone labels auto-position** right/left/bottom/top, hide if crowded
6. **Dependencies route smartly** - S-curve for right destinations, vertical for left

### UI Patterns
- **Floating windows**: Stack-based, modal, handle input/rendering
- **Text inputs**: Cursor tracking, scroll x for long text
- **Calendar picker**: Month navigation, today button, clear button
- **Toolbar buttons**: 5 icon buttons (today, add task, add milestone, users, settings)
- **Picture caching**: Home + back button, invalidated on hover/navigate/resize

### Coordinates
- **Physical pixels**: Window native resolution
- **Logical pixels**: DPI-independent, scaled by `scale_factor`
- **Gantt coordinates**: offset by header height, centered vertically if short content

### Common Gotchas
- Task/milestone labels show on light text on dark bars, dark on light
- Milestone label priority: Right → Left → Bottom → Top
- Dependencies don't edit on Plan Start marker (read-only)
- User schedule overrides fall back to default schedule
- Calendar overrides (user > plan > weekday schedule)

