# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build
cargo build --release
cargo run
cargo check          # fast compile check, no binary
cargo clippy
cargo test
cargo test <filter>  # run a single test or module, e.g. `cargo test data::task`
```

`cargo clippy` must be run after any code change. All clippy warnings must be resolved before considering work complete.

Tests live in `#[cfg(test)]` modules within each source file.

## Architecture

This is a desktop GUI application using OpenGL + Skia for rendering. The stack is: **winit** (window/event loop) → **glutin** (OpenGL context) → **skia-safe** (2D rendering).

The codebase has two largely independent layers: a **data/domain model** (`src/data/`) and a **rendering/UI layer** (`src/ui/`, `src/pages/`, `src/graphics/`). The data layer is fully implemented and tested; the UI layer renders from it but no scheduler has been written yet.

### Data model (`src/data/`)

The core entity is `Plan`, which owns everything:
- `tasks: HashMap<TaskId, Task>` and `milestones: HashMap<MilestoneId, Milestone>`
- `users: HashMap<UserId, User>` with skill/role tags for affinity matching
- `default_schedule: WorkSchedule` (hours per weekday) with per-user overrides
- `calendar: CalendarOverrides` (per-date hour exceptions, e.g. bank holidays) with per-user overrides
- `dates: StartDates` — separately stored computed start dates for tasks/milestones
- `start_date: NaiveDate` — the plan's root anchor, referenceable as `DependencyId::PlanStart`

**Task** has `workload_days: HashMap<UserId, f32>` (per-user effort), `duration_days: f32` (calendar span, 0 = derive from workload), `dependencies: Vec<Dependency>`, `required_tags`, and a `constraint: Option<DateConstraint>`.

**Dependency** edges carry `lag_days: f32` (positive = delay, negative = lead/overlap). `DependencyId` can reference a `Task`, `Milestone`, or `PlanStart`. Cycle detection runs on every `Plan::add_task_dependency` / `add_milestone_dependency` call.

**Capacity resolution** in `Plan::hours_available(user, date)`: user calendar override → plan calendar override → user schedule → plan default schedule.

**Storage** (`storage.rs`): versioned JSON snapshots under `$XDG_DATA_HOME/<binary>/plans/<plan-uuid>/YYYY-MM-DDTHH-MM-SS.json`. `Storage::from_user_data_dir()` for production, `Storage::from_path(tmp)` in tests.

**Not yet implemented**: a scheduler that computes `StartDates` from the dependency graph, workloads, and capacity.

### Rendering pipeline

The app uses a **retained-mode partial redraw** strategy:
1. A GPU-backed off-screen `Surface` (`retained_surface`) holds the last fully-rendered frame.
2. `DirtyRegion` (`None` | `ToolbarOnly` | `PageOnly` | `All`) tracks what needs repainting each event cycle.
3. On `RedrawRequested`, only dirty regions are re-rendered to `retained_surface`, then composited to the framebuffer via a cheap GPU blit.
4. The toolbar is additionally cached as a Skia `Picture` (display list) and only re-recorded when it changes (hover, active page, resize).

### UI module structure

- **`src/main.rs`** — Creates the winit `EventLoop`, initializes OpenGL via `graphics::setup`, constructs `Application`, and runs the loop.
- **`src/app.rs`** — `Application` struct implementing `ApplicationHandler`. Owns all state: GL env, render cache, page manager, dirty tracking, retained surface, toolbar picture cache.
- **`src/graphics/`** — OpenGL/Skia context setup (`setup.rs`) and the `Env` struct (`env.rs`) holding the window, GL surface/context, Skia `DirectContext`, and framebuffer-backed `Surface`.
- **`src/ui/`**
  - `layout.rs` — All layout constants (toolbar height, button sizes) and color palette as `u32` hex.
  - `toolbar.rs` — Toolbar drawing (`draw_toolbar`) and icon path builders (`build_icon_*`). Hit-testing via `hit_test_button`.
  - `cache.rs` — `RenderCache`: pre-built `Path` array for icons and `TextBlob`s for labels. Created once at startup.
  - `dirty.rs` — `DirtyRegion` enum with `merge` logic.
- **`src/pages/`** — Each page (`daily`, `planning`, `settings`) is a module with three files: `mod.rs` (struct + `Page` trait impl), `state.rs` (page-specific state), `render.rs` (Skia drawing logic). The `Page` trait requires `render`, `on_cursor_moved`, and `on_mouse_input`. `PageManager` owns all page instances and dispatches to the active one.

### Adding a new page

1. Create `src/pages/<name>/mod.rs`, `state.rs`, `render.rs` following the existing pattern.
2. Add a `PageId` variant and register the page in `PageManager`.
3. Add a toolbar button and wire `handle_button_click` in `app.rs`.

### skia-safe API notes

- Build paths with `PathBuilder`, not `Path::new()`. Call `.detach()` or `.snapshot()` to get a `Path`.
- Create typefaces with `FontMgr::new().match_family_style(...)` or `.legacy_make_typeface(...)`. There is no `Typeface::from_name()`.
- `Font::default()` works as a fallback.
