# Client and server stack

Decisions for the web client and the server it talks to. The scheduler is in `scheduler.md`.

## Requirements

- Everything Plinko 1 did (`plinko/plinko-web`): Gantt with zoom, pan and drag, allocation and
  resource views, calendar, forms, admin, live sync. It had only `react` and `uuid` as runtime
  dependencies and hand wrote every widget, so no UI library is being given up.
- Everything v2 adds: nested groups (moving an item is a parent change), buffers, targets and
  priorities, rule based availability with a live effective calendar, affinities, recorded workers,
  forecasts and delays, unscheduled reasons, bottleneck loads, tenant wide views, roles and groups,
  history and revert.
- Mobile is required, accessibility is nice to have, plans reach a thousand tasks, collaborative
  editing is required (every client viewing a plan sees every update), one developer.

## Stack

- **Client:** Leptos, client side rendered, built with trunk, installable as a PWA. No SSR, the app
  is behind a login. Pre 1.0 churn is accepted. Plain CSS with breakpoints.
- **Server:** axum, WebSocket for sync, one process per tenant.
- **Shared code:** the client depends on the `plinko` crate, so the rule engine, date maths,
  validation (cycles, parents) and `Plan::apply_update` are the same code on both sides, nothing
  is mirrored in another language. The crate must keep compiling for `wasm32` (uuid and getrandom
  features, no `std::time`).
- **Wire:** serde with msgpack or postcard, not JSON, because enum keyed maps like
  `HashMap<Worker, Workload>` do not survive JSON.
- Rejected: React or Svelte (types and the rule engine duplicated in TS), egui (forms and
  accessibility), Dioxus (its native targets are webviews anyway), htmx (no Gantt or dragging).

## Sync

- A tenant is one actor taking updates in order. A client sends a `TenantUpdateType`, the server
  runs `Tenant::stage` (fresh ids, defaults), logs it, applies it, and broadcasts the staged
  `TenantUpdate` with its sequence number to every subscriber, the sender included.
- Clients keep a replica of what they may see and apply broadcasts with `apply_update`, so replicas
  agree by construction. Late joiners take a snapshot and the log tail.
- No optimistic edits at first: a drag shows a local ghost and sends one update on release, the
  result is applied when it comes back. This avoids temp ids, since the server replaces the ids of
  adds. Conflicts are last writer wins per patched field, in server order.
- Broadcasts are filtered by plan access, the same rule as `Tenant::authorize_plan`.
- The schedule is the costly part. It runs off the actor in a blocking task and coalesces: updates
  that arrive during a solve are covered by the next one. Data updates go out at once, the schedule
  follows tagged with the sequence number it reflects, and clients show it as recalculating
  meanwhile. The schedule is tenant wide, so each client is sent only the slice it may see.
- Presence (who is viewing) can come later.

## Storage and auth

- One SQLite file per tenant, `data/<name>/tenant.db`, holds the update log, snapshots (every N updates, replay
  from the last), credentials and sessions. Users are already tenant state, so there is no
  global user table.
- The log holds only staged updates, no secrets. Password hashes are in a separate `credentials`
  table keyed by user id, never in updates or snapshots.
- Passwords use argon2id (memory hard, no 72 byte limit, unlike bcrypt). Login is by email.
- Sessions are an opaque random token in an `HttpOnly`, `Secure`, `SameSite=Lax` cookie. Only a
  hash of the token is stored, with an expiry. The WebSocket upgrade is authenticated by the cookie
  and checks `Origin`.

## Sharding

- A tenant is a process with its own database, so tenants never share memory or storage.
- A front proxy (nginx) routes a tenant's subdomain to its process. systemd runs one instance of
  a template unit per tenant, see `release.md`. There is no cross process coordination.

## Views

- The Gantt is one canvas in the style of Plinko 1 (`plinko1-ui.md`), on desktop and phones alike:
  packed lanes, group outlines, hover dependency lines, momentum drag, pinch and wheel zoom.
- Items open in a pane beside the chart. A "my work" agenda comes from `UserSchedule` (per day, across
  plans), and a plan can also be shown as a list.
- Everything except the Gantt canvas is real DOM elements, which is most of the accessibility.

## Status

All five steps are built: serde and the `wasm32` check (`check.sh`), the benchmark, the SQLite store
(`server/src/store`), the server (`server/src/net`) and the client (`client/`). Left to do:

- The schedule is sent whole to every client after each solve (about 300 KiB at a thousand tasks),
  which could be sent as a difference.
- Dragging bars on the Gantt, and the data blocks of tasks, are not in the client.
- Reverting a change, and a history view, need the log exposed to the client.
- Brute force limits on login, and a way to make a tenant without the environment variables.
