# Plinko 2

The second version of the Plinko project management tool, built around adaptive scheduling of
whole tenants. Design notes and progress are in `../PLAN.md`, the scheduler spec is `SCHEDULER.md`.

## Server

`cd server && cargo test && cargo clippy --all-targets`

- `types/` entities (tasks, milestones, groups, buffers, tags, affinities, data blocks) and shared value types.
- `availability/` rule sets that describe daily availability or allowed days, built into calendars.
- `planner/` plans, their updates and the dependency graph checks.
- `multitenancy/` tenants, login users, user groups and `TenantUpdate`.
- `scheduler/` the state a scheduler run writes. The solver itself is not written yet.
- `traits/` the shared `Entity`, `Patchable` and `Identified` traits.

## Conventions

- All state changes are a `TenantUpdate` (actor, time and resolved ids) applied with `Tenant::apply_update`.
  A server handler must call `TenantUpdateType::assign_ids` on anything a client sends first.
- Updates are applied in order and failed ones change nothing, so a log of successful updates rebuilds a tenant.
- Volatile state (calendars, the schedule) is derived and never saved.
