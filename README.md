# Plinko 2

The second version of the Plinko project management tool, built around adaptive scheduling of
whole tenants. Design notes and progress are in `../PLAN.md`, the scheduler spec is
`SCHEDULER.md` and the client and server stack is `CLIENT.md`.

## Layout

- `server/` the `plinko` crate: the model and scheduler (also compiled to WebAssembly for the
  client), and with the default `server` feature the store, the tenant server and its binary.
- `client/` the Leptos web client, built into `client/dist` by `client/build.sh`.
- `deploy/` one process per tenant: `tenant.sh`, a systemd template and an nginx example.

## Working on it

- `./check.sh` lints, tests, and checks the model and client build for the browser.
- `client/e2e/run.sh` runs the browser checks (needs node and chromium).
- `cargo run --release -p plinko --example bench` times scheduling a busy tenant.
- Run a tenant locally: `PLINKO_DB=tenant.db PLINKO_TENANT=Acme PLINKO_ADMIN_EMAIL=you@example.com
  PLINKO_ADMIN_PASSWORD=change-me-now PLINKO_INSECURE_COOKIES=1 cargo run -p plinko` after
  `client/build.sh`, then open http://127.0.0.1:3000. The admin settings are only read on the first
  run, the others are listed in `server/src/main.rs`.

## Source layout of the `plinko` crate

- `types/` entities (tasks, milestones, groups, buffers, tags, affinities, data blocks) and shared value types.
- `availability/` rule sets that describe daily availability or allowed days, built into calendars.
- `planner/` plans, their updates and the dependency graph checks.
- `multitenancy/` tenants, login users, user groups and `TenantUpdate`.
- `scheduler/` schedules a whole tenant (`scheduler::schedule`, spec in `SCHEDULER.md`) and holds the state it writes.
- `protocol.rs`, `codec.rs` the WebSocket messages and the format of updates, snapshots and schedules.
- `store/` a tenant's SQLite file: update log, snapshots, passwords and sessions.
- `net/` the tenant actor, the scheduling thread, and the HTTP and WebSocket front.
- `traits/` the shared `Entity`, `Patchable` and `Identified` traits.

## Conventions

- All state changes are a `TenantUpdate` (actor, time and resolved ids) applied with `Tenant::apply_update`.
  `Tenant::stage` assigns the ids and defaults of anything a client sends, and the server logs what it staged.
- Updates are applied in order and failed ones change nothing, so a log of successful updates rebuilds a tenant.
- Volatile state (calendars, the schedule) is derived and never saved.
