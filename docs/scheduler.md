# Scheduler

`scheduler::solve(&mut Tenant, &Hints) -> Solved` schedules every plan in a tenant together, because
people work across plans. `scheduler::schedule` does it for a tenant and keeps the result on it.

## 1. Principles

- Every item is placed. Hard dates slip as little as possible, nothing is left out to keep one.
- A task is worked in one run: days nobody can work it (weekends, holidays, days an affinity
  disallows) may fall inside it, days other work holds its people may not.
- Results do not depend on memory layout or `HashMap` order. Ties break by id. A search ends on a
  budget of placements, so the same tenant gives the same schedule. The one exception is the time
  limit (section 7).
- Failures are reported as unscheduled items or constraint violations, never as an error.

## 2. Inputs and output

Read from `Tenant`: people's availability (`Tenant::calendar`, cached by `refresh_calendars`), each
plan's default availability (`Tenant::plan_calendar`, used for its placeholder, its buffers and
tasks nobody works), group members, plan users and tags, the plan items, targets, priorities, and
`Tenant::today()`.

Written: `TenantVolatileState { plans, users }` (`scheduler/state.rs`).

- Per plan: `tasks` (date range, hours per person per day, the person for each worker),
  `task_groups`, `milestones` (date, derived status), `plan_buffers`, `plan_delays`, `unscheduled`
  with a reason, `constraint_violations`, `targets` and `plan_end` forecasts.
- `TargetForecast { forecast, baseline }`: the baseline is the earliest the goal could be if nobody
  ever ran out of hours (section 5), `delay()` the days after it. Both are days as a milestone
  on the goal would be shown (date conventions below), as is a milestone's `earliest`.
- `ConstraintViolation { constraint, end, scheduled, earliest }`: `end` says whether it is about the
  first or the last day. A miss no later than `earliest` could not have been avoided.
- `users[user]`: their work across plans and `bottleneck`, booked hours over available hours from
  their first to last booked day.

`Solved` also carries a `Solution` (section 8).

### Date conventions

- `DateRange` ends are exclusive. Durations are days, allocations 0 to 8 hours a day.
- Work that has not started can start today, on hours work in progress leaves free.
- An item's dependencies are the nodes whose end it waits for. Besides them, an item in a group
  waits for the group to start, and a top level item with no dependencies starts with the plan,
  today. Neither is listed.
- A milestone is shown on the last day of what it depends on, never before today. Once all of
  that has happened it is on the last day of the work that happened before it, or today if none
  did. What follows it starts the day after, or today if nothing has to happen first.
- A time constraint on a task or group is for its first or its last day (`NodeConstraint`). A
  milestone's is for its one day.

## 3. Node semantics

- **Tasks:** every worker has their hours on each of `duration` days. `Worker::User` is that
  person, `Tag` any plan user holding the tag, `UserGroup` any member of the group or the groups
  below it, `Placeholder` unlimited on the plan's default availability. A day counts when every
  worker has their hours on their own calendar and every affinity allows it. A task nobody works
  takes working days of the plan's default availability.
- **Statuses:** not started is scheduled. In progress starts on its actual start whatever it
  depends on, has its hours on every day it could be worked up to today (so it can run past its
  duration), then what is left of its days. Completed and on hold are fixed to their actual days
  and hold no hours on any of them. Dropped is unscheduled (`Dropped`) and what depends on it
  starts where it would have. In progress work is placed before anything that has not started, so
  it has first claim on today.
- **Recorded workers** (`Task.actual_workers`) replace the worker they are recorded for. They are
  filled in from the last schedule when a task starts (`Tenant::fill_defaults`).
- **Milestones** take no time. Their status is derived (`scheduler/status.rs`).
- **Task groups** start when their own dependencies are done, which their members wait for, and
  cover their first member's start to their last member's end. With no members they last 3
  calendar days. A group with members none of which is scheduled is unscheduled.
- **Delays** take calendar days.
- **Buffers** take working days of the plan's default availability. Unset, they end that many days
  after they start. Set, they end where they would have had they started on `set_start_day`, so
  the start floats. They never end before they start, and shrink to the hard start dates of the
  tasks and milestones that depend on them directly.
- **Time constraints:** an earliest date holds the item back. An earliest last day holds the work
  itself back, for a group its last members (those nothing else in it depends on). Latest and
  fixed dates are hard dates: missing one is a violation.
- **Unscheduled reasons:** `Dropped`, `NoEligibleWorkers` (a tag or group resolves to nobody),
  `NoCapacity` (no days within the furthest horizon, or one person asked for more than 8 hours),
  `DependsOnUnscheduled` (it lists dependencies and none of them is scheduled, or its group has
  not started), `NoScheduledMembers`. An item with one dependency scheduled and another not is
  placed after the one that is. Its group having started does not count as a dependency that is
  scheduled.

## 4. The network

`lower.rs` turns the tenant into one `Network` for all plans, in plan then id order.

- A **vertex** is a day boundary: "these things are done". Its time is the latest of what it waits
  on, and no earlier than its `earliest`. Its `anchor` is the start of the group it is in, which it
  waits for without that making it scheduled. Nodes with the same dependencies share one vertex, so
  what belongs to one node is only on a vertex that node owns: a milestone, and the start of a
  group with an earliest start. `deadline` is a hard date, which limits what comes before without
  moving anything. Each plan has an end vertex that waits on everything in it.
- An **arc** takes days between two vertices. `Work` is `days` days that every slot has its hours
  on, with floors and hard dates for its start and end. A delay is work with no slots on the full
  calendar. `Buffer` is the one arc whose length is not fixed.
- Days are offsets from the table's first day (`Day`), and a vertex time `d` is the boundary
  before day `d`: a constraint on a first day is the boundary before it, on a last day the
  boundary after.

`table.rs` holds hours per day for every person, plan default and the full calendar, and the days
each set of affinities allows. `Table::valid` gives the days work can be done on as a bit set that
answers "the n-th day from here" by counting (`bits.rs`). `ledger.rs` holds the hours still free
and finds the first, or last, run of days that no other work breaks up.

## 5. A pass

`pass.rs` builds one schedule. Work that has happened goes in first. A vertex settles when all it
waits on is settled, and arcs that take nobody's hours (delays, buffers, placeholder work) follow at
once. Arcs that take people's hours become ready and are placed one at a time, lowest key first,
each as early as it fits.

The key of an arc is the latest it may start, in days, for the hard dates it leads to, then for
targets, then for its plan's end, then how few days it could be worked on at all. Latest starts
come from `bound.rs`: a pass where nobody runs out of hours gives the earliest everything could be
(`earliest`), and working back from the due dates over those spans gives the latest starts
(`prioritise`). A target and a plan's end are due at their earliest. Priority is in days: a plan's
and a target's are taken off the due date.

A tag or group slot goes to whoever ends the work soonest, the person less asked for by other work
where two end together, never someone already on it.

## 6. Objective

`objective.rs`, lower is better and an earlier part always outweighs a later one:

1. work that found no days
2. days hard dates are missed by, then how many are missed
3. the latest goal, then all goals together: days after the earliest it could be, plus the plan's
   and target's priority. The goals of a plan are its targets, or its end if it has none.
4. the end days of all work added up

## 7. Search

`search.rs` scores a pass, then tries changes in rounds of up to three at once (on threads) and
keeps the best of a round if it is better:

- **Flips.** Walking back from what is late over whatever decided each start gives the arcs that
  held hours others were waiting for. A flip makes such an arc wait until the one it held up is
  placed, or gives a slot to the next best person.
- **Sooner.** Every missed hard date counts as due sooner by the days it was missed, so the work
  leading to it goes earlier.

Every try is justified (`justify.rs`): everything is pushed as late as it can go without a goal
ending later, then placed again as early as it can go in that order, and the better of the two is
kept. A try that differs from the last by one flip starts from a snapshot of it.

A search may place 30 arcs for each arc that takes people's hours, and stops after 1 second
whatever is left. Only the second can make two runs differ. What a search kept comes back as
`Hints`, which the next search starts from if they still help.

## 8. Solutions and storage

A `Solution` is what a search decided: where each task that takes people's hours starts and who
works it, and the hints. `scheduler::expand` puts every task there and lets the rest follow, with no
search, so it gives the same schedule every time.

The store (`store/mod.rs`) keeps every update, a snapshot of the tenant each time the day changes,
and the solution after each schedule, as what changed since the last or whole. The tenant and its
schedule as of any update are read back with `load_at` and `schedule_at`. The server starts from a
saved solution without solving when the tenant has not changed since.

## 9. Performance

`cargo run --release --example rich [seeds] [plans] [tasks] [people]` makes tenants with part time
people, half days, affinities, tags and groups, and checks each schedule is valid. Against the
scheduler this replaced, on the same tenants (`DATES=dir`):

| Tenant | | Before | Now |
|---|---|---|---|
| 4 plans of 1000 tasks, 40 people | hard dates missed, days late | 18.5, 116 | 0.5, 0.5 |
| | time | 3.9 s | 0.2 s |
| | plan end with no hard dates | 698 days | 456 days |
| 4 plans of 250 tasks, 20 people | hard dates missed, days late | 1.3, 3.7 | 0.3, 0.3 |
| | time | 0.6 s | 0.1 s |
| | plan end with no hard dates | 293 days | 245 days |

`rich` takes its hard dates from the scheduler's own schedule without them, so a scheduler that
packs plans tighter sets itself harder dates. With its own dates at 4 plans of 1000 tasks this one
misses 165 of 382, which more budget does not improve.

## 10. Plan rules beside scheduling

### Redundant dependencies

After an update that changes an item's dependencies or group, the server follows it with updates
that drop what other dependencies already imply: `a -> c` goes when `a -> b -> c` exists and the
chain leads there (today, a group's start and a group's end count
as links). Both the update and the follow-ups are logged, so replays do not recompute it
(`Plan::simplifications`).

### Availability rules

A rule set (a person's hours, a plan's placeholder hours, an affinity's days) always holds a
default weekly schedule from 1970-01-01 that cannot be removed or moved, only its days changed.
Rules start on a day and are applied in order: weekly schedules (later start wins), day overrides
(`days` calendar days from the start, optionally every `n` days, weeks, months or years until the
stop day) and holidays, which must have a stop day. Rules may start between 2000 and 2100.

Every `n` months or years counts from the start day by day number, so a rule on the 31st falls on
the last day of a shorter month (Feb 29, Mar 31). Every `n` months aligned goes by the weekday
instead: a rule that starts on the second Monday lands on the second Monday of each month, or with
"align from month end" on the second last. A month that has no such weekday that many times uses its
last one.

### Removing items

`Plan::removal` gives the updates that remove an item without breaking chains: whatever depended
on it depends on what it depended on. A removed group moves its members to the
group's parent, with the group's dependencies as the start they had, and whatever depended on the
group depends on each member. Targets on the item go too.

### Moving between groups

`Plan::moving(node, parent)` gives the updates. Entering a group: what the item waited on moves onto the group (except what is inside it), what waited on the item from outside waits on the group's end, and the item waits on the group's start. Leaving: the item's group-start dependency is replaced by the group's own dependencies. A group cannot move into itself.
