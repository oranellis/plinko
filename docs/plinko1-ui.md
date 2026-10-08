# Plinko 1 UI

What the first version's web client (`plinko/plinko-web`) looked like and how it behaved, written
down so the second version's UI can be steered back towards it on purpose. Everything here is read
from the source, with the file it comes from. Sizes are CSS pixels. It is a React 19 + TypeScript
+ Vite app with no UI or chart libraries: `react`, `react-dom` and `uuid` only. Every widget, the
date picker and the dropdowns included, is hand written, and the three big views draw on a canvas.

## Shape of the app

- One WebSocket to the server for everything. The client holds one **active plan** at a time, and
  a plan switcher in the toolbar loads another (`LoadPlan`). v1 had organisations and plans, not
  tenants: a login belongs to organisations, the active organisation decides which plans are listed.
- Navigation is a `page` value in a React context, not a URL (`context/PlanContext.tsx`). Pages:
  `home`, `overview` (the Gantt), `allocation`, `resources`, `settings`, `admin`, and a `daily`
  stub that only says "Daily". Only `/admin` is a real path. `CalendarPage.tsx` is not routed
  any more, Resources replaced it.
- `App.tsx` stacks a **toolbar** over a **page area** (`display:flex; flex-direction:column;
  height:100vh; overflow:hidden`). Pages fill the page area and scroll inside themselves, so the
  window itself never scrolls (`html, body {overflow:hidden}`), and the viewport meta forbids pinch
  zoom (`maximum-scale=1.0, user-scalable=no`).
- Before the shell there are full screen states: connecting (spinner, the socket URL, "taking
  longer than expected" after 8 s, a Reconnect button after 10 s), login, disconnected (Reconnect
  button), protocol error, and "No plan active" (logo, title, an **Open Settings** button, the
  signed in email with an `admin` badge and **Sign out**). Each shows the protocol version
  bottom left in `#444`.
- A **toast**, "Plan updated by another user", appears bottom right for 3 s when someone else
  changes the plan (`remote-update-toast`, fades in over 10%, out over the last 30%).

## Theme

Dark only, no light mode and no theme switch. Colours are literals repeated through the CSS and the
canvas code, not variables. The ones that carry the look, by role:

| Role | Colour |
|---|---|
| Page and canvas background | `#1e1e1e` (Home page, panels, toolbar menus use `#252526`) |
| Raised surfaces, headers, modals, cards | `#252526` |
| Alternate row | `#222224` / `#202022` |
| Hover fill | `#2d2d30`, list items `#37373d` |
| Border / divider | `#3a3a3c`, finer lines `#2a2a2c` and `#2a2a2a` |
| Text | `#d4d4d4`; secondary `#888`/`#999`; faint `#555`/`#666`; titles `#e8e8e8` |
| Accent (primary buttons, focus, today, links) | `#4a90d9` (hover `#5aa0e9`, disabled `#2d5580`) |
| Selected row | `#2d4a6a` (lists) or `#1c3a5a` with `#7cb9f4` text (plan switcher) |
| Danger / error | `#e53935` buttons, `#e57373` text, `#e05c5c` field borders |
| Warning / constraint violation | `#e65100` (the `!`), tooltip text `#e8a87c` |
| Settings icon when active, Plan Start | purple `#a78bfa` (hover `#c4b5fd`) |
| Scheduler target / "gold" | `#ffd600` outline, `#f5c842` text |

Task and milestone state colours (`utils/planUtils.ts`):

| Status | Bar colour |
|---|---|
| Not started | `#555555` (grey) |
| In progress | `#f5a623` (amber) |
| On hold | `#b39ddb` (lilac) |
| Complete | `#66bb6a` (green) |
| Dropped | `#8b7355` (brown) |

Milestone diamonds use their derived status: complete `#66bb6a`, in progress `#f5a623`, otherwise
`#888888`. The login page is darker than the rest: page `#0f0f0f`, card `#1a1a1a` with a
`#2a2a2a` border, radius 12, padding 40, width 360. The Resources page uses `#1a1a1a`.

- **Type:** `system-ui, -apple-system, sans-serif` on the page, `font-family: inherit` on every
  control, and `sans-serif` on the canvases. Root size 16 px, but most UI text is set in px:
  13–14 body, 12 labels and buttons, 16 modal titles and toolbar title, 24 for the big page titles.
  Form labels are 12 px, uppercase, letter-spaced `0.04em`, `#999`.
- **Shape:** radius 4 on buttons and inputs, 6 on modal panels and dropdowns and canvas panels, 8
  on toast and mobile cards, 12 on the Home cards and login card. Borders are 1 px. Shadows only
  on floating things: `0 8px 24px rgba(0,0,0,.6)` dropdowns, `0 8px 32px rgba(0,0,0,.6)` modals.
- **Buttons** (`.btn`): padding `7px 18px`, 14 px, no border; `-primary` `#4a90d9`, `-danger`
  `#e53935`, `-secondary` `#3a3a3c` (hover `#4a4a4c`), `-sm` is `4px 12px` at 12 px. Icon
  buttons in the toolbar are borderless, gain `#2d2d30` and a `#3a3a3c` border on hover.
- **Inputs:** background `#1e1e1e`, border `#3a3a3c`, text `#d4d4d4`, 14 px, padding `7px 10px`,
  border turns `#4a90d9` on focus, number spinners hidden. Numeric fields use a text input with
  digit filtering (`filterNumericKey`) rather than `type=number`.
- **Segmented control:** connected buttons in a 1 px `#3a3a3c` frame, selected `#4a90d9` white
  bold, others `#3a3a3c`, hover `#4a4a4c`, height 30 (26 small). Used for Status and Constraint.
- **Icons:** hand drawn inline SVG, 16 px viewBox, `currentColor`, round caps, about 1.8 stroke;
  shown at 24 in the toolbar and 36 on the Home cards. Back, Gantt, Allocation, Calendar,
  Settings, Resources, Users, Search, Today, Add task, Add milestone, Home, Monday pull and push,
  Spinner, and a bug for bug reports.
- **Brand:** the logo is a plinko board of three rows of shiny pegs (3-2-3) in warm greys, 80 px on
  the login and empty screens, also the favicon.
- **Motion:** deliberately little. Sidebar width transitions 0.18 s ease (and the Allocation panel
  is animated by hand over the same 180 ms), hover fills 0.1–0.15 s, connecting spinner, toast
  fade, the search flash on the Gantt, momentum scrolling.

## Layout of the shell

**Toolbar** (`components/Toolbar.tsx`, `.css`): 44 px tall, background `#1e1e1e` (the same as the
pages, with no border line), `flex-shrink:0`, hidden on the connecting, login, disconnected and
error screens.

- **Centre:** the plan name as a button, absolutely centred (`left:50%`), 16 px / 500, max 40% wide,
  ellipsised, with a small `▾`. Clicking it opens the **plan switcher**: a portal dropdown 240 px
  wide under the title, `#252526`, border `#3a3a3c`, radius 6, max 320 tall, a `✓` before the
  current plan (row `#1c3a5a`, text `#7cb9f4`), "Loading plans…" and "No plans available"
  states, closes on outside click or Escape. The title reads "Loading…" while a plan loads.
- **Left:** a back chevron (hidden on Home, returns to the page you came from when leaving
  Settings, otherwise to Home), then the **page's own actions**. Pages register their buttons
  through `setToolbarActions` and clear them on exit, so the toolbar changes with the page.
- **Right:** page right actions, Monday pull and push (only when the plan has a Monday
  integration, with a spinner while running, and a status bar that floats under the toolbar
  right aligned for 5 s, 8 s for errors), a bug report button, the Settings gear (purple when
  on Settings, toggles back), and an 8 px connection dot: green connected, orange connecting or
  handshaking, red disconnected or error.
- **Phones (≤640 px):** the toolbar becomes a column, title above the buttons, 14 px, centred,
  padding `8px 10px 6px`.

**Home** (`pages/HomePage.tsx`): a centred row of three 160×160 cards, 24 gap, `#252526`, border
`#3a3a3c`, radius 12, hover `#2d2d30` and a `#4a90d9` border. Each has a 36 px icon over a 14 px
label: **Overview**, **Allocation**, **Resources**. The user email, `admin` badge and Sign out sit
bottom right, the version bottom left. At ≤530 px the cards stack as 280×64 rows, icon left of the
label.

## The Gantt (Overview)

`pages/OverviewPage.tsx` (1029 lines) with `utils/planUtils.ts`. One full-bleed canvas, no DOM
inside it. It is the heart of v1 and differs from the first v2 Gantt in a few ways that matter: there
is **no label column**, rows are **packed** with several items per row, and the time axis is
**anchored to the plan start**.

### Dimensions

| | |
|---|---|
| Row height | 36 |
| Header | 56: month row 22, day row 34 |
| Day width | default 34, min 8, max 80 |
| Bar | row inset `0.16 × row` top and bottom (about 6, so a 24 high bar), `0.12 × day width` left and right |
| Corner radius | 6 |
| Milestone diamond | radius 10 (Plan Start 9), centred on the day column |

The canvas fills its container and is resized with a `ResizeObserver`, scaled by
`devicePixelRatio`. Redrawn from React state on any change, hit rectangles are rebuilt on every
draw.

### Coordinates and scroll

- `x = daysBetween(plan.start_date, date) × dayW − scrollX`. Day 0 is the **plan's start date**, so
  nothing before the plan starts has a column, except that `scrollX` may go down to `−width/2`
  (half a screen of empty space before day 0). The right limit is the latest end plus half a screen.
- Vertical scroll is in whole pixels between 0 and `rows × 36 − (height − 56)`. Header and rows
  scroll together horizontally, and the header stays put vertically (rows are clipped below it).
- On opening a plan (and on **Jump to today**) the view centres today: `scrollX = max(0, todayOffset
  × 34 − width/2)`, `scrollY = 0`. The zoom resets to 34 on Jump to today only.

### Rows are packed

`packGanttRows` places every task and milestone into rows by greedy bin packing:

1. **Plan Start** is a synthetic milestone, always first, in row 0.
2. Active items (all but dropped) are sorted by start date then name.
3. For each item, take the first row whose last item ended before it, i.e. `item.start >
   rowEnd` where a row's end is the last item's end **plus one day**, so items in a row are
   separated by at least one blank day. Otherwise open a new row.
4. **Dropped** tasks are not in that packing. They are packed on their own **below a separator
   row**, a thin horizontal rule `rgba(138,138,138,.31)` across the full width, with no labels.

So a row is a lane: one lane can hold a whole run of short tasks and milestones end to end, and a
plan with many small tasks is much shorter than one row per task. There is no grouping or
nesting (v1 has no task groups), and nothing says which row a task is in.

### What is drawn, back to front

1. Background `#1e1e1e`.
2. **Today column:** a full height band `rgba(64,91,200,.18)`, one day wide, behind everything.
3. **Header** (`#252526`, bottom border `#3a3a3c`).
   - Month row: `Mon YYYY` in 13 px `#888` at the start of each month, a short divider at each
     month boundary.
   - Day row: each day gets a cell, **weekends in `#1e1e1e`** (darker than weekdays' `#252526`)
     with the day number in `#555` (weekday `#aaa`), today's number `#4a90d9`. Numbers are
     only drawn when a day is at least 18 px wide.
   - A 1 px vertical line `#2a2a2c` from the day row to the bottom of the canvas on **every**
     day. There are no horizontal row lines and no zebra rows, and weekends are not shaded
     in the body, only in the header.
4. **Dependency lines** for the hovered item only (below).
5. **Items.**
   - **Task:** a rounded bar over the task's span in its status colour, the name inside it at
     **16 px**, `rgba(255,255,255,.9)` (black `.85` on the amber in-progress bar), clipped to the
     bar. Names show as `name | context label` when a context label exists. Bars narrower than
     the text simply clip it.
   - **Leaf indicator:** a task or milestone nothing depends on gets a blue gradient over its
     right 28 px (`rgba(30,136,229,0)` to `.65`), marking the ends of chains.
   - **Milestone:** a diamond centred in its day column, then the name to its right at 14 px
     `#bbb` (`#f5d040` when hovered), clipped at the start of the next item in the same row.
   - **Plan Start:** a purple diamond with the label "Plan Start".
   - **Scheduler target:** a 3 px gold `#ffd600` outline on its bar or diamond.
   - **Constraint violation:** a bold orange `!` (`#e65100`, 16 px) just right of the bar, or
     between a diamond and its name, which has its own hover tooltip.
6. **Hover panel** (below).

### Hover and dependencies

Hover only exists for a mouse. Hovering an item:

- Outlines it `#1e88e5` (3.5 px), outlines everything it **depends on** magenta `#fc1ef1` and
  everything that **depends on it** cyan `#07fcd7` (3 px), and draws a straight 1.5 px line from
  each predecessor's end day centre to the hovered item's start day centre in
  `rgba(252,30,241,.66)`, and from the hovered item to each dependent in
  `rgba(7,252,215,.66)`. Dropped tasks are skipped. There are **no permanent arrows**: the graph
  only shows for the item under the pointer.
- Shows an **info panel** drawn on the canvas, bottom left, 8 px in from the corner: radius 6,
  fill `rgba(30,30,30,.86)`, border `#4a4a4a`, a soft offset shadow, 10 px padding. The first line is
  the name at 20 px `#d4d4d4`, the others 15 px `#8a8a8a`:
  - task: `Status: …`, `Started: …` (actual start) or `Scheduled: …`, `Ends: …`, `Workers: …`
    (user names, or `needs: tag, tag`, or `(unassigned)`), and `⚠ Constraint violated`.
  - milestone: `Milestone`, `Scheduled: …`, the violation line.
  - Plan Start: `Date: …`.
  - A small Monday logo and tick on the second line for items linked to Monday.
- The `!` marker's tooltip is separate: a small panel by the pointer with an orange border and
  three lines, `Fixed date violated` / `Latest date exceeded` / `Earliest date not met`,
  `Required: …`, `Scheduled: …`.
- The cursor is a pointer over an item, otherwise the default.

### Interaction

- **Drag** pans in both directions, mouse or touch, with **momentum**: on release the last
  velocity decays by 0.85 per frame until under 0.5 px/s. A press that moves less than 4 px
  (10 px for touch) is a **tap**, not a drag.
- **Wheel** pans (deltaX and deltaY). **Shift + wheel zooms** by 0.9/1.1 per notch, keeping the day
  under the cursor still. There is **no pinch zoom** and no zoom buttons, and plain scrolling
  never zooms.
- **Tap or click** an item opens its edit modal (task form or milestone form). Plan Start and the
  `!` markers do not open anything.
- **Toolbar actions** on this page: Jump to today, Add task, Add milestone, Search.
- **Search** is a modal with an autofocused filter ("Filter tasks and milestones…", substring on
  the display name, results sorted by name, `▬` for tasks and `◇` for milestones, the box
  border flashes red when nothing matches). Choosing a result closes it, scrolls to centre that
  item on both axes after a short wait, and **flashes its outline red** (500 ms on/off, three
  times, then fades over 700 ms).
- Nothing is draggable: dates are not edited by moving bars. Changing a plan means editing in the
  modal and the scheduler redraws it.

## Allocation

`pages/AllocationPage.tsx`: who is doing what, per day, for one person. A canvas too.

- **Left panel** 220 px wide (`#252526`): a "USERS" header and the plan's users by name in 32 px
  rows, selected row `#2d4a6a`, alternating `#252526` and `#222224`. A ☰/✕ button at the top left
  collapses it, the width animates over 180 ms, and it **starts collapsed below 600 px** and
  collapses itself after you pick someone on a narrow screen.
- **Timeline** to its right with a two row date header (month 20, day 24, 28 px days) like the
  Gantt, the same day grid and the today band, then a **utilisation row** (36 px): for each day
  the person's total hours as a fraction of 8, coloured green under 80%, orange up to 100%, red
  above, with the hours.
- Below that, one 32 px row per **task of the selected user** (not complete or dropped, by start
  date, scrolled so the task nearest today is in the middle). In each row a bar per working day,
  its height the fraction of 8 hours, each task its own colour from a ten colour palette
  (`#4a90d9 #7ed321 #f5a623 #d0021b #9b59b6 #1abc9c #e67e22 #2ecc71 #e74c3c #3498db`, by the
  task's place in a sorted list), with the hours (`4h`) over a dark pill. A task with no hours is a
  dashed line across its span with its dates. The task name floats at the left edge of the
  timeline on a translucent pill, max 200 wide. A hover panel and the orange `!` work as on the
  Gantt, and tapping a task opens its form.
- Mouse drags scroll sideways only, touch drags lock to one axis after 12 px, the wheel scrolls
  the list and sideways.

## Resources

`pages/ResourcesPage.tsx` (DOM, not canvas): the people, tags and working hours.

- **Left panel** 300 px (240–340), collapsible (starts collapsed under 600 px) with `☰`:
  a "Plan" entry (the plan's default schedule) and the user list (about 40% of the height),
  each row with a **drag handle** for reordering, the name, their tags, and an edit button that
  shows on hover. Under it, the **weekly hours** editor (Monday to Sunday, in two columns) and the
  **tag list** (drag to reorder, inline rename, delete, add). Create user and edit user are
  forms in the same panel (name, tag chips, Delete / Cancel / Save).
- **Right:** a vertically scrolling **calendar**, 6 months back to 18 months ahead, a sticky Mo–Su
  header, a block per month (current month's label highlighted), cells with the day number and the
  hours: today's cell `#1a2a1a` with a green number, overridden days `#1e2840` showing e.g. `4h`
  in a different colour, and **pending** edits outlined orange `#c97c2a`. Clicking a day opens a
  popup (date, hours input 0–24 in half steps, Clear, OK). **Today** scrolls back.
- Edits to the schedule, tag names and calendar overrides are **buffered locally** and sent when
  you change selection or leave the page, with "n unsaved" shown by the calendar.

## Settings and admin

- **Settings** (`pages/SettingsPage.tsx`): a left nav of 180 px (collapsible, a `☰` to bring it
  back) with **Profile** (email, change password), **Plan** (name, start date, scheduler target
  picker), **Plan management** (Save, New plan, the list of saved plans with Delete, Integrations:
  Monday, Version history with Restore), **User links** (which login is which plan user) and
  **Organisation** (members and roles, per-plan permissions, shown to org admins), with an
  organisation selector at the bottom and a "Site administration →" link for site admins.
- **Admin** (`pages/AdminPage.tsx`, also reachable at `/admin`): a single scrolling page of
  sections: login users (list, create), organisations (create, members, add member, per plan
  permissions) and bug reports.
- **Login** (`pages/LoginPage.tsx`): the card described above, logo, "Plinko", "Sign in to
  continue", email and password fields, an error line, a button.

## Forms and modals

- **Modal** (`components/Modal.tsx`): a backdrop `rgba(0,0,0,.6)` over everything (z-index 1000),
  panel `#252526`, border `#3a3a3c`, radius 6, default width 520 (task form 500, search 420), max
  90vh and `100vw − 32px`, a header with the title and a `×`, a scrolling body. Escape closes,
  clicking the backdrop closes, and **Enter saves** unless a textarea is focused.
- **Task form** (`TaskFormModal.tsx`), in order: name, description (4 rows), **status** (segmented:
  Not started, In progress, On hold, Complete, Dropped), **duration in days** and **mode** (a
  Strict/Relaxed toggle button, blue when strict) side by side, **constraint** (segmented None,
  Earliest, Fixed, Latest, and a date picker), **actual start** and **actual end** side by side
  (enabled by status, labelled red with `*` when required), **workers**, **dependencies** and
  **"Required by"** (the dependents, editable from the other end). Buttons: Delete (left, only on
  edit), Cancel, Save.
  - Moving to Complete fills the actual end with today, to Dropped fills it from the planned
    dates, and going back clears what was filled automatically.
- **Milestone form:** name, description, constraint, dependencies and Required by.
- **Worker editor:** rows with a `T`/`P` toggle (tags, or a specific person), a picker for the
  user or the tags (a multi select of tags, with "Any (no tags required)"), and the workload in
  days. A new row defaults to a tag worker with one day.
- **Dependency editor:** a bordered list showing 3½ rows (36 each), each row a target picker, a lag
  input (positive delays, negative leads) and `×`, empty text "No dependencies added yet", a `+`
  button that opens the picker on the new row. Items sort by name, Plan Start purple, the target
  gold.
- **Floating picker** and **date picker** are portal popups placed below their field, or above if
  there is no room, filtered by typing (picker) or with `‹ ›` months, `« »` years, **Today** and
  **Clear** (date picker, no native date input).
- Edits go in on **Save**, there is no autosave in the forms. Errors from the server show as a
  red line at the top of the modal.

## Behaviour worth keeping in mind

- Everything is request and response over the socket (`sendRequest`), and the server pushes the
  whole updated plan back, so the Gantt always draws what the scheduler last produced.
- The session token is kept in `localStorage` and reused on reconnect, and there is no automatic
  reconnect: a dropped connection shows the Disconnected screen and the user reconnects by hand.
- Plans are saved explicitly (Plan management, **Save**) and versioned, with an admin only Restore.
- Monday.com sync, the bug report modal, and the organisation features are v1 only and are not
  being carried over for now.

## What the first v2 UI does differently

For steering: the points where the first v2 build departs from v1.

- **Theme:** v2 is a light page with a dark top bar, v1 is dark throughout.
- **Gantt rows:** v2 gives every item its own row with a 220 px name column on the left, v1 packs
  items into shared lanes with names on the bars and no name column.
- **Time axis:** v2 starts at the earliest scheduled date and clips at the first and last, v1
  counts from the plan start with half a screen of slack either side.
- **Bars and markers:** v2 colours bars by status in a bright blue family and shows
  weekends across the body, v1 uses the grey/amber/lilac/green status colours, shades weekends only in
  the header, shows a gold target outline, a leaf gradient and the orange `!`.
- **Dependencies:** v2 draws none, v1 draws the hovered item's predecessors and dependents.
- **Editing:** v2 edits in a side panel (a sheet on phones) with native controls, v1 uses centred
  modals with hand drawn pickers and a segmented status control.
- **Navigation:** v2 has a top tab bar (a bottom bar on phones) and routes in the URL, v1 has the
  Home cards and a toolbar whose buttons belong to the page.
- **Gestures:** v2 pans by drag, pinches, and zooms with Ctrl + wheel, v1 has momentum, and zoom on
  Shift + wheel only.
- **Search, Jump to today** and the red flash on found items exist in v1 only.
