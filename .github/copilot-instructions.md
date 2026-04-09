# Copilot Instructions

## Commands

```bash
cargo build
cargo build --release
cargo run
cargo check                  # fast compile check, no binary
cargo clippy
cargo fmt                    # format the entire codebase
cargo test
cargo test <filter>          # run a single test or module, e.g. `cargo test data::task`
```

**After every code change**: run `cargo fmt` (whole codebase) and `cargo clippy`. All clippy warnings must be resolved.

**After every feature, fix, or chore**: create a git commit using [Conventional Commits](https://www.conventionalcommits.org/) format:
```
<type>(<scope>): <short description>
```
Common types: `feat`, `fix`, `refactor`, `chore`, `docs`. Scope is the module or area changed (e.g. `ui`, `data`, `scheduler`). Commit after `cargo fmt` and `cargo clippy` pass. Then push with `git push`.

Each distinct change (feature, fix, refactor) must be its own commit — never bundle two unrelated changes into one commit, even if done in the same session.

Always include this trailer in every commit message:
```
Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
```

Tests live in `#[cfg(test)]` modules within each source file.

## Architecture

Desktop GUI app: **winit** (window/event loop) → **glutin** (OpenGL context) → **skia-safe** (2D rendering).

### Workspace structure

Three crates:
- **`plinko-shared/`** — data model, scheduler, storage, protocol types (no UI dependencies)
- **`plinko-ui/`** — rendering, pages, UI widgets, engine client
- **`plinko/`** — binary entry point; spins up the server and connects the UI

### Client–server split (`plinko-ui/src/engine.rs`)

The app communicates with an in-process server over TCP using newline-delimited JSON (`plinko-shared/src/protocol.rs`). **All Plan mutations go through `PlanRequestSender`** — call `.send(PlanRequest::Foo {...})`. The server applies the change and broadcasts a `PlanState` snapshot back; the UI replaces its cached `Plan`.

Page-to-engine communication uses `pending_*` fields on the page's `State` struct. `app.rs` drains these each event cycle and calls `engine.sender().send(...)`. Example pattern:
```rust
pub pending_save: bool,
pub pending_load: Option<Uuid>,
pub pending_set_user: Option<Option<UserId>>,
```

### Data model (`plinko-shared/src/data/`)

Core entity is `Plan`:
- `tasks: HashMap<TaskId, Task>`, `milestones: HashMap<MilestoneId, Milestone>`
- `users: HashMap<UserId, User>` with skill/role tags
- `default_schedule: WorkSchedule` (hours per weekday) with per-user overrides
- `calendar: CalendarOverrides` (per-date hour exceptions) with per-user overrides
- `dates: StartDates` — separately stored computed start dates
- `start_date: NaiveDate` — root anchor, referenceable as `DependencyId::PlanStart`

**Task**: `workers: Vec<WorkerSlot>`, `duration_days_target: f32` (0 = derive from workload), `dependencies: Vec<Dependency>`, `required_tags`, `constraint: Option<DateConstraint>`, `relaxed_mode: bool` (false = strict: all workers must share the same working days).

**Dependency** edges carry `lag_days: f32` (positive = delay, negative = lead/overlap). Cycle detection runs on every `add_task_dependency` / `add_milestone_dependency` call.

**Capacity resolution** in `Plan::hours_available(user, date)`: user calendar override → plan calendar override → user schedule → plan default schedule.

**Storage**: versioned JSON snapshots under `$XDG_DATA_HOME/<binary>/plans/<plan-uuid>/YYYY-MM-DDTHH-MM-SS.json`. Use `Storage::from_path(tmp)` in tests.

### Rendering pipeline

Retained-mode partial redraw:
1. `retained_surface` (GPU-backed off-screen) holds the last fully-rendered frame.
2. `DirtyRegion` (`None | BackButtonOnly | PageOnly | All`) tracks what needs repainting per event cycle.
3. On `RedrawRequested`, only dirty regions are re-rendered, then composited to the framebuffer via GPU blit.
4. Toolbar is additionally cached as a Skia `Picture` (display list), re-recorded only when it changes.

**Every event handler returns `DirtyRegion`** — never trigger redraws directly.

### Page system (`plinko-ui/src/pages/`)

Each page is a module with three files: `mod.rs` (struct + `Page` trait impl), `state.rs` (mutable state), `render.rs` (Skia drawing). Pages: `home`, `daily`, `allocation`, `overview`, `calendar_overrides`, `settings`.

`Page` trait requires: `render`, `on_cursor_moved`, `on_mouse_input`, `on_key_input`, `reset_hover`, `take_open_request`. Optional: `on_scroll`, `tick_animation`.

**`render(&self, ...)` is immutable.** Use `RefCell` fields on `State` to cache values produced during rendering (e.g. hit-test rects) for later use by event handlers.

To open a modal from a page: set an internal flag in `on_mouse_input`, then return the `Box<dyn FloatingWindow>` from `take_open_request`. `Application` calls this immediately after `on_mouse_input`.

### Floating windows (`plinko-ui/src/ui/floating_window.rs`)

Modal overlays use the `FloatingWindow` trait. `FloatingWindowManager` maintains a stack — events route to the topmost window. Handlers return `FloatingWindowOutcome { dirty, close }`. Use `FloatingWindowOutcome::close()` to dismiss; the manager pops it automatically.

Child windows are pushed the same way: set an internal flag, return the new window from `take_open_request`.

Default Escape-to-close is provided by the trait's default `on_key_input`.

### Adding a new page

1. Create `src/pages/<name>/mod.rs`, `state.rs`, `render.rs` following the existing pattern.
2. Add a `PageId` variant and register in `PageManager`.
3. Add a toolbar button and wire `handle_button_click` in `app.rs`.

## Key Conventions

**Colors**: stored as `0xAA_RRGGBB` `u32` hex constants in `plinko-ui/src/ui/layout.rs`, converted via `Color::from(value)`. All layout constants (sizes, gaps, padding) live there too.

**Skia text baseline**: `canvas.draw_str` / `canvas.draw_text_blob` Y is the *baseline*, not the top. To place text with its top at `y`:
```rust
let draw_y = y - metrics.ascent; // ascent is negative, so this adds |ascent|
```
To vertically center text of `font` in a box of height `h` starting at `box_top`:
```rust
let text_h = metrics.descent - metrics.ascent;
let draw_y = box_top + (h - text_h) / 2.0 - metrics.ascent;
```
**Never use `blob.bounds().height()` as a vertical offset** — it includes both ascent and descent, pushing text ~descent pixels too low.

**Skia paths**: use `PathBuilder`, not `Path::new()`. Call `.detach()` or `.snapshot()` to get a `Path`.

**Skia typefaces**: use `FontMgr::new().match_family_style(...)` or `.legacy_make_typeface(...)`. There is no `Typeface::from_name()`. `Font::default()` works as a fallback.

**`TextInput`** (`plinko-ui/src/ui/text_input.rs`): the scroll offset field uses `Cell<f32>` for interior mutability so the render function can update it without `&mut self` on the containing struct. Always use `handle_key(key, modifiers) -> bool` and `handle_paste(text)` instead of manually handling individual keys — they cover Backspace, Delete, Ctrl+arrow word-jump, Home, End, and character insertion consistently. `handle_key` returns `true` if consumed; Tab/Enter/Escape are left for the window to handle.

**`MultiLineInput`** (`plinko-ui/src/ui/multi_line_input.rs`): similarly use `handle_key(key, modifiers, inner_width, line_h, visible_h, font)` and `handle_paste(text, ...)`. These take extra layout params needed for cursor scrolling.

**`FloatingWindow::on_key_input`** takes `modifiers: &Modifiers` as a parameter (needed for Ctrl+arrow word navigation). All window implementors must accept and forward it.

**`TaskPatch` / `MilestonePatch`**: use chainable setters to build partial updates. `Option<Option<T>>` fields follow the pattern: `Some(None)` clears, `Some(Some(v))` sets, `None` leaves unchanged.

**Render-time caching**: when `render` (immutable `&self`) must produce data for hit testing, store it in `RefCell<Vec<Rect>>` (or similar) on the page's `State`. Populate in `render`, consume in `on_cursor_moved` / `on_mouse_input`.

**Engine mutations and scheduling**: any `PlanRequest` handler in `plinko/src/engine.rs` that modifies plan data affecting the schedule (dependencies, dates, workers, scheduler_target, start_date) must call `self.plan.compute_time_optimised_plan()` before returning `PlanResponse::PlanUpdated`.

**Task/Milestone context labels**: `Task` and `Milestone` both have `#[serde(default)] pub context_label: Option<String>`. This is populated from Monday.com (group name for top-level items, parent item name for subitems) when `MondayConfig::show_monday_context` is true. Display it as `"{name} | {context}"` using a local `display_name(name, ctx)` helper wherever task/milestone names are rendered.
