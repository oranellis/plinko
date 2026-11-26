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

- One SQLite file per tenant, `data/<name>/tenant.db`, holds the update log, a snapshot of the tenant for each
  day (loading replays from the last), what the scheduler decided after updates, credentials and
  sessions. Users are already tenant state, so there is no
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
- Items are created and edited on the chart, there is no pane. A plan can also be shown as a list, from which an item opens on the chart.
- Home shows your tasks in progress and the next ones to start, with buttons to start or finish them,
  and your favourite plans. Allocations is a table with a row for each of your tasks across plans and the hours you work on
  each day, under a row of the hours you have; a task there opens its plan with the task selected. Availability opens your own calendar.
- Availability, and the days an affinity allows, are edited from a calendar (`rulecalendar.rs`): a
  window from plan settings, a full page for your own hours. Dates and week start follow the
  preferences of the browser.
- Everything except the Gantt canvas is real DOM elements, which is most of the accessibility.

## Status

All five steps are built: serde and the `wasm32` check (`check.sh`), the benchmark, the SQLite store
(`server/src/store`), the server (`server/src/net`) and the client (`client/`). Left to do:

- The schedule is sent whole to every client after each solve (about 300 KiB at a thousand tasks),
  which could be sent as a difference.
- Dragging bars on the Gantt, and the data blocks of tasks, are not in the client.
- Reverting a change, and a history view, need the log exposed to the client.
- Brute force limits on login, and a way to make a tenant without the environment variables.

## Gantt selection

Tapping an item selects it: its dependency lines stay on show and a bar opens below it with the item's buttons. Tasks get a status switcher (the button is the next status, hovering lists all five), buffers a set/reset start button, and all but buffers a data button (description plus one block of each kind: to-do, links, budget). The pencil opens the edit pane. While the item is edited the bar slides to the bottom middle of the chart, out of the way of the handles.

### Creating and editing

Everything set of an item while it is created or edited is staged in a `Draft` (`draft.rs`) and reaches the plan only on save; a new item is not in the plan until then. Redundant links are dropped by the server on save. On the chart what is staged shows as the usual magenta and cyan outlines, a blue outline on its group, and changed days as `+2 (5d)` above the item.

A new item has a card over the bottom of the chart with its shape (`card.rs`):

- Left and right circles: what it waits for and what waits for it. Drag one onto an item to link it, or click it to light it and then click items to link or unlink them. A link that would make a loop is refused with a notice.
- Middle grip: drop on a group, on an item to join its group, or on blank space for the top level; or click it and then click.
- Right edge: the day count. Drag sideways to change it (slower under six days), click to type it. The shape never changes length.

On touch nothing on the card drags, so the chart still pans: handles are tapped and days typed. A second click puts a lit handle out.

An item in the plan has no card: while it is edited its bar is drawn without its name and its shift handles (below) show without shift, and what they do is staged (`chart_edit::staging`). Clicking one of its dots, without dragging, leaves it in its colour: clicks on other items then link or unlink them until it is clicked again. The days a started task really ran are staged too, as how far each is moved, and show above it as `start -1  end +2`. Where its edges are moved to is a ghost over it, which is where its handles then are.

Both have the same strip, in the card for a new item and faded into the docked bar for one in the plan (`context.rs`): the name, typed over where it stands (enter saves), then workers and affinities in a window for a task, save, cancel and delete. Leaving with changes asks first (`guard.rs`).

A task, group or milestone in the plan can be held to a day. While it is edited a timer stands under each of its ends (a milestone has one), faint until that end is held: a click holds the end to the day it is scheduled on, takes the day over from the other end, or lets it go. The held day has a band down the chart like today's, and by the item's row a tick on the edge that is held with arrows: on from it for no earlier, back from it for no later, both at it for exactly. Its handle is in its cell of the header: dragged to another day, or clicked, after which a click on any day moves it there and buttons under it change how it is held or let it go. An item that is hovered or selected shows the band and arrows faintly.

### Shift editing

A top level item with no dependencies shows a purple wash on its left edge while hovered, like the blue one on the right of an item nothing depends on. Plan settings name a default start node (a picker with search) that new top level items depend on to begin with; the plan has no start date to set.

With shift held, items the pointer is near show handles: a dot off each edge (drag to another item; right edge: it depends on this item, left edge: this item depends on it), a dot in the middle (drop on a group to move in, on another item to join its group, on blank space for the top level, see `Plan::moving`) and, for tasks and buffers, a grip inside the right edge that changes the duration by whole grid days, shown as `+2 (6d)`. Dragging to the chart's edge scrolls it. Shift-clicking an item linked to the selected one cuts that dependency. With an item selected, hovering another one on a dependency chain with it shows the longest chain in red.

Handles are grey; a dependency dot takes magenta (left, what comes before) or cyan (right, what comes after) when taken. For started tasks (in progress, on hold, completed) dragging the middle handle sideways moves the days they really ran, unless it is dropped on another item, and the right grip changes the end date of on hold, completed and dropped ones (in progress ones change duration as normal). A grip inside the left edge of a started task changes the day it started.

### Drawing the chart

The canvas is drawn once per animation frame however many signals changed. Two layers are kept as
images (`gantt/layers.rs`): what is behind the items (days, today, groups) and the items, each drawn
bigger than the screen so scrolling only moves them, and drawn again when the chart changes, the
zoom changes or the screen nears their edge. What the pointer changes is drawn over them every frame:
dependency lines between the layers, then outlines and the names of groups and milestones, the header
and the notes. `Pen` remembers canvas settings so unchanged ones are not sent to the browser again.
