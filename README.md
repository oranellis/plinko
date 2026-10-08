# Plinko 2

The second version of the Plinko project management tool, built around adaptive scheduling of
whole tenants. Design notes and progress are in `../PLAN.md`.

## Two ways to run it

- **Debug**, to work on it: `./run_debug.sh` builds the debug client and server and runs a tenant
  on http://127.0.0.1:3000, with its files in `data/Debug`. See `docs/development.md`.
- **Release**, to host it: `deploy/build.sh`, `deploy/install.sh` and `deploy/new_tenant.sh` build
  it, install it under `/opt/plinko` and run a systemd service for each tenant, each keeping its
  files in `/opt/plinko/data/<name>`. See `docs/release.md`.

In both the server keeps a tenant's files in `data/<name>` under the folder it runs from, which
is ignored by git.

## Layout

- `server/` the `plinko` crate: the model and scheduler (also compiled to WebAssembly for the
  client), and with the default `server` feature the store, the tenant server and its binary.
- `client/` the Leptos web client, built into `client/dist` by `client/build.sh`.
- `deploy/` the release path: build and install scripts, the systemd template, an nginx example.
- `docs/` `development.md`, `release.md`, the scheduler spec `SCHEDULER.md` and the client and
  server stack `CLIENT.md`.
- `run_debug.sh` the debug path, and `check.sh` which lints, tests and checks the browser build.

## Source layout of the `plinko` crate

- `types/` entities (tasks, milestones, groups, buffers, tags, affinities, data blocks) and shared value types.
- `availability/` rule sets that describe daily availability or allowed days, built into calendars.
- `planner/` plans, their updates and the dependency graph checks.
- `multitenancy/` tenants, login users, user groups and `TenantUpdate`.
- `scheduler/` schedules a whole tenant (`scheduler::schedule`, spec in `docs/SCHEDULER.md`) and holds the state it writes.
- `protocol.rs`, `codec.rs` the WebSocket messages and the format of updates, snapshots and schedules.
- `store/` a tenant's SQLite file: update log, snapshots, passwords and sessions.
- `net/` the tenant actor, the scheduling thread, and the HTTP and WebSocket front.
- `traits/` the shared `Entity`, `Patchable` and `Identified` traits.

## Conventions

- All state changes are a `TenantUpdate` (actor, time and resolved ids) applied with `Tenant::apply_update`.
  `Tenant::stage` assigns the ids and defaults of anything a client sends, and the server logs what it staged.
- Updates are applied in order and failed ones change nothing, so a log of successful updates rebuilds a tenant.
- Volatile state (calendars, the schedule) is derived and never saved.
