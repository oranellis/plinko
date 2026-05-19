# Plinko — Agent Guide

Plinko is a project-management tool built as a **Rust backend + React web frontend**.  
The backend is a multi-crate Cargo workspace; the frontend lives in `plinko-web/` and is a standalone Vite/React app.

---

## Commands

### Rust (run from repo root)

```bash
cargo build                     # build all crates
cargo build --release           # release build
cargo run                       # run the server (port 7892 by default)
cargo check                     # fast type-check, no binary
cargo clippy                    # lint — all warnings must be resolved
cargo fmt                       # format entire workspace
cargo test                      # run all tests
cargo test <filter>             # run a single test, e.g. `cargo test strict_multi_worker`
```

**Mandatory after every code change**: run `cargo fmt` (whole workspace), then `cargo clippy`. All clippy warnings must be resolved before committing. If any Rust source files are changed (including `Cargo.toml` dependency changes), always stage and commit `Cargo.lock` in the same commit.

### Frontend (run from `plinko-web/`)

```bash
npm run dev -- --host           # dev server with network access (port 5173)
npm run build                   # tsc -b && vite build → dist/
npm run lint                    # eslint
npx tsc --noEmit                # type-check without emitting
```

After `cargo build --release`, the server looks for `plinko-web/dist/` and serves it on port+1 (default 7893).

---

## Workspace Structure

```
plinko-shared/   # data model, scheduler, storage, protocol types — no UI deps
plinko/          # binary: WebSocket server, auth, Monday.com integration, engine
plinko-web/      # React SPA: UI pages, components, hooks
```

### `plinko-shared/src/`

| Module | Purpose |
|--------|---------|
| `data/plan.rs` | `Plan` aggregate root; owns all tasks, milestones, users, tags, calendar data |
| `data/scheduler.rs` | Core scheduling algorithm — computes `NodeAllocations` from the dependency graph |
| `data/allocation.rs` | `TaskAllocation` (Fixed vs Dynamic), `WorkSegment`, `TaskState`, `NodeAllocations` |
| `data/storage.rs` | Versioned JSON snapshots; `Storage::from_path(tmp)` in tests |
| `data/task.rs` | `Task`, `WorkerSlot` (Specific / Placeholder) |
| `data/dependency.rs` | `Dependency` (target node + `lag_days`) |
| `data/ids.rs` | `TaskId`, `MilestoneId`, `UserId`, `TagId` (newtype UUIDs); `NodeId` enum |
| `protocol.rs` | All client↔server message types: `PlanRequest`, `PlanResponse`, `ServerMessage`, `ClientMessage` |

### `plinko/src/`

| File | Purpose |
|------|---------|
| `main.rs` | Starts static file server (port+1) and WebSocket server |
| `ws_server.rs` | Accepts TCP connections; spawns per-session threads; owns `SessionRegistry` |
| `server.rs` | `handle_protocol()` — all request routing and response logic |
| `engine.rs` | `PlanEngine::apply_request()` — mutates plan, calls scheduler |
| `auth.rs` | SQLite auth DB (`auth.db`): sessions, bcrypt passwords, plan visibility |

### `plinko-web/src/`

| Path | Purpose |
|------|---------|
| `hooks/usePlan.ts` | WebSocket lifecycle, auth state, request/response correlation |
| `context/PlanContext.tsx` | Provides plan state, routing (`PageId`), toolbar slot injection |
| `App.tsx` | Page router, disconnected screen, remote-update toast |
| `pages/` | One file (+ CSS) per page: `HomePage`, `OverviewPage`, `AllocationPage`, `ResourcesPage`, `DailyPage`, `SettingsPage` |
| `components/modals/` | All floating form dialogs (`TaskFormModal`, `MilestoneFormModal`, etc.) |
| `components/Modal.tsx` | Base modal with Escape-to-close and Enter-to-save |
| `protocol.ts` | TypeScript mirror of `plinko-shared/src/protocol.rs` — must be kept in sync manually |

---

## Architecture

### Client–Server Protocol

All communication is **newline-delimited JSON over WebSocket** (port 7892).

1. Client sends `ClientMessage::Request { id, request: PlanRequest }`.
2. Server responds with `ServerMessage::Response { id, response: PlanResponse }`.
3. If the response is `PlanResponse::PlanUpdated`, the server **immediately** sends a follow-up `ServerMessage::PlanState { plan, has_monday_integration }` — this is the canonical post-mutation state.
4. `PlanState` is also sent unsolicited to all *other* connected sessions (multi-session broadcast via `SessionRegistry`).

The protocol version string (`VERSION` / `PROTOCOL_VERSION`) must match between `plinko-shared/src/protocol.rs` and `plinko-web/src/protocol.ts` or the connection is rejected.

### Engine Mutations

`engine.rs::PlanEngine::apply_request()` handles plan-data mutations.  
**Every handler that modifies schedule-relevant data must call `self.plan.compute_time_optimised_plan()` before returning `PlanResponse::PlanUpdated`.**  
"Schedule-relevant" means: tasks, milestones, workers, dependencies, dates, start_date, scheduler_target.

Server-level requests (LoadPlan, NewPlan, auth, Monday, visibility) are handled with `if let` chains in `server.rs` before reaching `engine.rs`. These are listed in the exhaustive `match` at the end of `engine.rs::apply_request` as a catch-all returning `PlanResponse::PlanUpdated` without side effects.

### Multi-Session Broadcast

`ws_server.rs` maintains a `SessionRegistry` (`Arc<Mutex<HashMap<u64, Sender<ServerMessage>>>>`).  
Each session registers its sender on connect and is deregistered via `RegistryGuard` (RAII) on disconnect.  
After any mutation, `broadcast_plan_state()` in `server.rs` fans out the `PlanState` snapshot to all other sessions. **Conflict policy: last-writer-wins** (serialised through `Arc<Mutex<PlanEngine>>`).

### Auth System (`plinko/src/auth.rs`)

- Passwords stored as bcrypt hashes (cost 12) in SQLite (`auth.db`).
- Sessions use opaque random tokens stored in `sessions` table; `authenticate_token()` validates.
- **Plan visibility**: empty `plan_visibility` rows = visible to all. Non-empty = restricted to listed user IDs + all admins.
- Default admin account `root@plinko.local` / `root` is created on first run.

### Scheduler (`plinko-shared/src/data/scheduler.rs`)

`Plan::compute_time_optimised_plan()` runs the full scheduling pipeline:

1. **Pre-insert anchored tasks** — Fixed/InProgress tasks with real segments are locked into `SchedulerState` capacity first so they are not displaced.
2. **Topological scheduling** — Tasks and milestones are inserted in critical-path order. For each task: resolve `WorkerSlot` → real users, compute `daily_cap`, call `fill_slot` (single worker) or `fill_slots_synchronized` (strict multi-worker).
3. **Compact passes** — Repeat until stable: pull tasks forward toward the `scheduler_target` if gaps open up, propagate dependency constraints.

**Key scheduling concepts:**

- **`duration_days_target`**: Calendar span override. `0.0` = derive from workload.
- **`relaxed_mode`**: `false` (default, "strict") — all workers scheduled on the same calendar days; a worker only fills a day when *all* workers have capacity. `true` ("relaxed") — each worker fills independently at full daily rate.
- **`WorkerSlot::Specific`** — named user with `workload_days` effort. **`WorkerSlot::Placeholder`** — resolved at schedule time to any user matching `required_tags`.
- **`effective_duration`** — Computed per-task before allocation:
  - Explicit target set: `max(ceil(target), max_workload_days_across_workers)`
  - No target, strict, multiple workers: `max_workload_days_across_workers` (ensures lighter workers spread over the heaviest worker's duration)
  - Otherwise: `None` (fill at full daily rate)
- **`daily_cap`** — `total_hours / effective_duration` per worker. Prevents lighter workers from finishing ahead of heavier ones.
- Start dates are always advanced to the next working day after constraint resolution.

### Frontend Data Flow

```
usePlan (WebSocket) → PlanContext → usePlanContext() in each page/component
```

- `sendRequest(PlanRequest)` sends a request and returns `Promise<PlanResponse>`.
- After any `PlanUpdated` response the server pushes a fresh `PlanState`; the hook sets `plan` state unconditionally.
- `ownPlanStateCountRef` tracks how many own-mutation `PlanState` messages are expected; any extra `PlanState` triggers the remote-update toast.
- Pages inject toolbar buttons via `setToolbarActions` / `setToolbarRightActions` from `PlanContext`.

---

## Key Conventions

### Rust

- **`TaskPatch` / `MilestonePatch`**: Chainable setters for partial updates. `Option<Option<T>>` fields: `Some(None)` = clear, `Some(Some(v))` = set, `None` = no change. Uses a custom `deserialize_optional_field` serde helper.
- **Tests**: `#[cfg(test)]` modules inside the source file they test (no separate test files). `Storage::from_path(tmp_dir)` for isolated storage in tests.
- **Adding a new `PlanRequest` variant**: Add to `protocol.rs` enum → add handler in `server.rs` (if server-level) **or** `engine.rs` (if plan mutation) → add to the exhaustive `match` catch-all at the bottom of `engine.rs::apply_request` → mirror the type in `plinko-web/src/protocol.ts`.
- **`context_label`**: `Task` and `Milestone` both have `#[serde(default)] pub context_label: Option<String>`. Display as `"{name} | {context}"` wherever names are rendered (populated from Monday.com group/parent names when `MondayConfig::show_monday_context` is true).

### Frontend

- **All plan mutations go through `sendRequest(PlanRequest)`** — never mutate local state directly; wait for the `PlanState` broadcast.
- **ResourcesPage deferred mutations**: Calendar overrides, tag renames, and schedule changes are buffered in refs and flushed on page exit (via `useEffect` cleanup). Immediate operations (user CRUD, tag add/delete, drag reorder) call `sendRequest` directly.
- **Modal Enter-to-save**: Pass `onSave={handleSave}` to `<Modal>`. The base modal fires `onSave` on Enter unless focused element is a `<textarea>` or `contenteditable`.
- **Number inputs**: Only allow `0-9` and `.` characters. Reject values > 24.0 for schedule hour boxes.
- **`protocol.ts`** is a manual TypeScript mirror of `protocol.rs`. Any new request/response variant added to Rust must be added here too.
- **NodeId JSON shape**: `"PlanStart"` (string literal), `{ Task: "<uuid>" }`, or `{ Milestone: "<uuid>" }`. Use `nodeIdString()` from `planUtils.ts` to convert to a stable string key.

### Storage

- Plans saved as versioned **MessagePack** snapshots: `$XDG_DATA_HOME/<binary>/plans/<plan-uuid>/YYYY-MM-DDTHH-MM-SS.msgpack`.
- Auth DB: `$XDG_DATA_HOME/<binary>/auth.db` (SQLite).
- `Storage::from_user_data_dir()` for production; `Storage::from_path(tmp)` in tests.

### Git / Commits

- Conventional Commits format: `<type>(<scope>): <description>`.
- Common types: `feat`, `fix`, `refactor`, `chore`, `docs`.
- Each distinct change gets its own commit — never bundle unrelated changes.
- Always include the trailer:
  ```
  Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
  ```

---

## Versioning

Plinko uses **semantic versioning** (`MAJOR.MINOR.PATCH`). One version number covers the Rust binary, the React bundle, and the WebSocket protocol — they are always deployed together.

### Bump rules

| Change | Bump |
|---|---|
| Breaking wire protocol or storage format change | MAJOR |
| New user-visible feature | MINOR |
| Bug fix, refactor, non-user-visible feature | PATCH |

### The four canonical locations — all must always match

| File | What to edit |
|---|---|
| `Cargo.toml` (workspace root) | `version = "X.Y.Z"` under `[workspace.package]` |
| `plinko-web/package.json` | `"version": "X.Y.Z"` |
| `plinko-shared/src/protocol.rs` | `pub const VERSION: &str = "X.Y.Z";` |
| `plinko-web/src/protocol.ts` | `export const PROTOCOL_VERSION = "X.Y.Z";` |

`plinko/Cargo.toml` and `plinko-shared/Cargo.toml` use `version.workspace = true` and inherit automatically — do not edit them.

### Helper script

```bash
./scripts/bump-version.sh 0.4.0
```

Updates all four files atomically and prints the next steps (check → commit → tag).

### Rules for AI sessions

**Every AI work session that makes code or config changes must bump the patch version as part of its final commit.** This ensures deployed versions are always distinguishable and stale browser clients reconnect cleanly after an upgrade.

Procedure at the end of each session:
1. Run `./scripts/bump-version.sh <next.version.number>` (e.g. `0.3.0` → `0.3.1`)
2. Run `cargo check` to verify the workspace still compiles
3. Stage the four version files and commit: `chore: bump version to X.Y.Z`
4. Do **not** create a git tag — tags mark human-approved releases

If a session introduces a new user-visible feature, bump MINOR instead of PATCH (e.g. `0.3.0` → `0.4.0`). If a session changes the wire protocol or storage format incompatibly, bump MAJOR.

### Git tags (human-only)

Tags mark releases and are created by the repository owner, not AI:

```bash
git tag v0.4.0
git push origin v0.4.0
```
