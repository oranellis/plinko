# Layout

## Folders

- `server/` the `plinko` crate: the model and scheduler (also compiled to WebAssembly for the
  client), and with the default `server` feature the store, the tenant server and its binary.
- `client/` the Leptos web client, built into `client/dist` by `client/build.sh`, and `client/e2e`
  the browser checks.
- `deploy/` the release path: build and install scripts, the systemd template, an nginx example.
- `docs/` these documents.
- `data/` where a tenant keeps its files, `data/<name>` under the folder the server runs from.
  Ignored by git.
- `run_debug.sh` the debug path, and `check.sh` which lints, tests and checks the browser build.

## The `plinko` crate

- `common/` errors, `Crud`, short ids and date ranges, shared across the crate.
- `availability/` rule sets that describe daily availability or allowed days, built into calendars, and `Workload`.
- `planner/` plans, their updates and the dependency graph checks. `items/` are the schedulable nodes (tasks, groups, milestones, buffers, delays) and `resources/` the rest of a plan (tags, targets, affinities, users, data blocks).
- `multitenancy/` tenants, login users, user groups and `TenantUpdate`.
- `scheduler/` schedules a whole tenant (`scheduler::schedule`, spec in `scheduler.md`) and holds the state it writes.
- `protocol.rs`, `codec.rs` the WebSocket messages and the format of updates, snapshots and schedules.
- `store/` a tenant's SQLite file: update log, daily snapshots, saved schedules, passwords and sessions.
- `net/` the tenant actor, the scheduling thread, and the HTTP and WebSocket front.
- `traits/` the shared `Entity`, `Patchable` and `Identified` traits.

## Conventions

- All state changes are a `TenantUpdate` (actor, time and resolved ids) applied with `Tenant::apply_update`.
  `Tenant::stage` assigns the ids and defaults of anything a client sends, and the server logs what it staged.
- Updates are applied in order and failed ones change nothing, so a log of successful updates rebuilds a tenant.
- Volatile state (calendars, the schedule) is derived and never saved.
