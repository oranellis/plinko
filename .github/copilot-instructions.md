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

Tests live in `#[cfg(test)]` modules within each source file.

Make sure to make and commit each feature rather than grouping features or fixes into a single commit

## Architecture

Desktop GUI app: **winit** (window/event loop) → **glutin** (OpenGL context) → **skia-safe** (2D rendering).

Two largely independent layers:
- **Data/domain model** (`src/data/`) — fully implemented and tested
- **Rendering/UI layer** (`src/ui/`, `src/pages/`, `src/graphics/`) — renders from data; no scheduler yet

### Engine (`src/engine.rs`)

**All Plan mutations go through `PlanEngine`**, not directly on `Plan`. The UI submits `PlanRequest`s via a clonable `PlanRequestSender`; the app main loop drains the queue each event cycle and acts on returned `PlanResponse`s. Mutations that could break an existing schedule are validated by cloning the plan, applying, and re-running the scheduler before committing.

### Data model (`src/data/`)

Core entity is `Plan`:
- `tasks: HashMap<TaskId, Task>`, `milestones: HashMap<MilestoneId, Milestone>`
- `users: HashMap<UserId, User>` with skill/role tags
- `default_schedule: WorkSchedule` (hours per weekday) with per-user overrides
- `calendar: CalendarOverrides` (per-date hour exceptions) with per-user overrides
- `dates: StartDates` — separately stored computed start dates
- `start_date: NaiveDate` — root anchor, referenceable as `DependencyId::PlanStart`

**Task**: `workload_days: HashMap<UserId, f32>`, `duration_days: f32` (0 = derive from workload), `dependencies: Vec<Dependency>`, `required_tags`, `constraint: Option<DateConstraint>`.

**Dependency** edges carry `lag_days: f32` (positive = delay, negative = lead/overlap). Cycle detection runs on every `add_task_dependency` / `add_milestone_dependency` call.

**Capacity resolution** in `Plan::hours_available(user, date)`: user calendar override → plan calendar override → user schedule → plan default schedule.

**Storage**: versioned JSON snapshots under `$XDG_DATA_HOME/<binary>/plans/<plan-uuid>/YYYY-MM-DDTHH-MM-SS.json`. Use `Storage::from_path(tmp)` in tests.

### Rendering pipeline

Retained-mode partial redraw:
1. `retained_surface` (GPU-backed off-screen) holds the last fully-rendered frame.
2. `DirtyRegion` (`None` | `PageOnly` | `All`) tracks what needs repainting per event cycle.
3. On `RedrawRequested`, only dirty regions are re-rendered, then composited to the framebuffer via GPU blit.
4. Toolbar is additionally cached as a Skia `Picture` (display list), re-recorded only when it changes.

**Every event handler returns `DirtyRegion`** — never trigger redraws directly.

### Page system (`src/pages/`)

Each page is a module with three files: `mod.rs` (struct + `Page` trait impl), `state.rs` (mutable state), `render.rs` (Skia drawing). Pages: `home`, `daily`, `overview`, `settings`.

`Page` trait requires: `render`, `on_cursor_moved`, `on_mouse_input`, `on_key_input`, `reset_hover`, `take_open_request`.

To open a modal from a page: set an internal flag in `on_mouse_input`, then return the `Box<dyn FloatingWindow>` from `take_open_request`. `Application` calls this immediately after `on_mouse_input`.

### Floating windows (`src/ui/floating_window.rs`)

Modal overlays use the `FloatingWindow` trait. `FloatingWindowManager` maintains a stack — events route to the topmost window. Handlers return `FloatingWindowOutcome { dirty, close }`. Use `FloatingWindowOutcome::close()` to dismiss; the manager pops it automatically.

Child windows are pushed the same way: set an internal flag, return the new window from `take_open_request`.

Default Escape-to-close is provided by the trait's default `on_key_input`.

### Adding a new page

1. Create `src/pages/<name>/mod.rs`, `state.rs`, `render.rs` following the existing pattern.
2. Add a `PageId` variant and register in `PageManager`.
3. Add a toolbar button and wire `handle_button_click` in `app.rs`.

## Key Conventions

**Colors**: stored as `0xAA_RRGGBB` `u32` hex constants in `src/ui/layout.rs`, converted via `Color::from(value)`. All layout constants (sizes, gaps, padding) live there too.

**Skia paths**: use `PathBuilder`, not `Path::new()`. Call `.detach()` or `.snapshot()` to get a `Path`.

**Skia typefaces**: use `FontMgr::new().match_family_style(...)` or `.legacy_make_typeface(...)`. There is no `Typeface::from_name()`. `Font::default()` works as a fallback.

**`TextInput`** (`src/ui/text_input.rs`): the scroll offset field uses `Cell<f32>` for interior mutability so the render function can update it without `&mut self` on the containing struct.

**`TaskPatch` / `MilestonePatch`**: use chainable setters to build partial updates. `Option<Option<T>>` fields follow the pattern: `Some(None)` clears, `Some(Some(v))` sets, `None` leaves unchanged.
