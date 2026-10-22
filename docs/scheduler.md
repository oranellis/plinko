# Scheduler

Specification for `scheduler::solve_tenant(&Tenant) -> TenantVolatileState`. It schedules every plan
in a tenant together, because people work across plans. Plinko 1's scheduler
(`plinko/plinko-shared/src/data/scheduler.rs`) is the reference for the engine. This document
records what is settled and what carries over from it, and lists the changes still needed in the
data model.

Status: the output state, availability calendars, affinities, groups, recorded workers, buffers,
`ScheduleMode`, `TimeConstraint::apply`, the ledger, baselines, the single-plan solver and the
combined insertion across plans and repair rounds exist. Call `scheduler::prepare` to cache calendars and baselines,
then `solve_tenant`. 
Items marked **Assumed** were not explicitly confirmed and are collected at the end.

## 1. Principles

- The scheduler is a pure function of tenant state. The same updates always produce the same dates.
  Memory layout and `HashMap` order do not matter, so results must not depend on iteration order.
  Break every tie by id (ids are random, so ties break arbitrarily but the same way every time).
- It writes volatile state only (derived, never saved, never part of an update). `Tenant` clears the
  stored schedule on every successful update, and the runner calls `solve_tenant` then
  `Tenant::store_schedule`. Persistent state is what users record about how a plan evolves
  (status, actual dates, the people who actually did the work).
- Prioritise completing each plan as fast as possible. Plans run in parallel, and priorities decide
  contention only.
- A task is never broken by priority. Once placed, a lower priority task cannot take hours from a
  placed one. Only calendar gaps (weekends, holidays) pause a task.
- Failures are reported as unscheduled items or constraint violations. The scheduler returns a
  result, never an error, for bad plan data.
- Search horizon is 3650 days (Plinko 1's `MAX_FILL_DAYS`) from the start of a search.

## 2. Inputs

Everything is read from `Tenant` and its plans.

- **People:** `TenantLoginUser.availability` (a `RuleSet<Workload>`), read as calendars. Use
  `Tenant::refresh_calendars(from, until)` then `Tenant::calendar(user)`. `until` is the scheduling
  horizon, `from` the earliest day any plan needs.
- **Placeholder availability:** `Plan.default_availability` through `Tenant::plan_calendar(plan)`.
  It is also the "plan calendar" for working-day arithmetic on that plan (task durations with no
  workers, buffers, group placeholders).
- **Groups:** `Tenant::group_members(group)` (a group and all subgroups) for `Worker::UserGroup`.
- **Plan membership and tags:** `Plan.users` (and each user's tag ids) for `Worker::Tag`.
- **Plan state:** tasks, milestones, buffers, task groups, affinities, `PlanSchedulerData`
  (priority, default start node), targets (see section 10).
- **Today:** `Tenant::today()`, one clock for every plan, moved only by `SetToday` updates (admin
  only, sent by the server runner).

## 3. Output

`TenantVolatileState { plans, users }`.

- `plans[plan]: PlanVolatileState` holds, per kind of node, only items that could be scheduled:
  - `tasks: task::VolatileState { date_range, assignments: Assignee -> day -> Workload, workers: Worker -> Assignee }`
    (`workers` is the person resolved for each worker the task asks for)
  - `task_groups: { date_range }`
  - `milestones: { date, derived_status: DerivedStatus }`
  - `plan_buffers: { date_range }`
  - `unscheduled: PlanNode -> UnscheduledReason`
  - `constraint_violations: PlanNode -> { constraint, scheduled }`
- `users[user]: UserSchedule` is built by `TenantVolatileState::from_plans` from the task
  assignments, so a user's view across plans (past, present, upcoming) cannot disagree. Placeholders
  have no schedule.
- `Assignee` is the concrete person (`User(id)` or `Placeholder`), never a tag or group.
- `plans[plan].targets[target]` and `plans[plan].plan_end` (the last day of the plan, the forecast of
  a plan with no explicit target): `TargetForecast { forecast, baseline }` with `delay()` in days, and
  `users[user].bottleneck`: booked hours over available hours from their first to last assigned day,
  set by the solver after `from_plans` (it knows availability).

### Date conventions

- `DateRange` end is exclusive: a task's range ends the day after its last working day. A
  zero-length range has `start == end`.
- Allocations are 0–8 hours per day (`Workload`). Durations are in days.
- A dependency carries no time. A wait is a delay item: calendar days from where its dependencies end, with no workload and no time constraint (`PlanDelay::place`).
- Time constraint dates are the start of the task or milestone (`TimeConstraint::apply(earliest)`).
- Unstarted tasks cannot start before tomorrow (Plinko 1; **Assumed**). Milestones may land on
  today. A milestone whose derived status is complete may land in the past (it already happened).
- A plan has no start date. `ParentStart` at the top level is today, and an item with no dependencies
  depends on `ParentStart` of its parent. A plan that starts later has a milestone not before then
  that its items depend on (the plan's default start node, which clients give new items). A
  derived-complete milestone with nothing but `ParentStart` has no day of its own and lands today.
- Every node has an exclusive end, the day after its last day. A milestone occupies the one day it
  lands on (`milestone::VolatileState::end()`). A task, group or buffer starts at a dependency's
  end, so a task after a milestone starts the day after it.
  A milestone lands on the last day of everything it depends on, tasks, groups, buffers and other
  milestones alike, less a day for the exclusive end: milestones are zero time events
  at the trailing edge of their real work, and share a day with milestones they depend on.
  `ParentStart` is a date, not a node, so the same applies to it for milestones.
  So task, milestone, task starts the second task the day after the first finishes, exactly as
  task, task does: the milestone is a zero cost marker on the last day of the first task's work.
- Dropped predecessors are transparent: they add no duration and hold no capacity, so dependents
  start as if the dropped task ended where it would have started.

## 4. Node semantics

### Tasks

- **Effort:** each worker slot works `allocation` hours per day for `duration` days, so the
  effort per worker is `hours * duration`. A duration is at least 1 day (the default, enforced by
  updates), there are no zero-length tasks. A task with no workers lasts `duration` working days of
  the plan calendar and has no assignments.
- **Workers:** `Worker::User(id)` is that person. `Worker::Tag(tag)` is any plan user holding the
  tag. `Worker::UserGroup(group)` is any member of the group, including subgroups, whether or not
  they are plan users. `Worker::Placeholder` is unlimited and never conflicts, but its hours per
  day are bounded by the plan default calendar's hours that day.
- **Start:** the first day on or after the computed start on which work is placed, using the
  workers' own availability (a Saturday override can start work, and nobody working Sunday means a
  task that could start Sunday starts Monday). A task with no workers starts on the first working
  day of the plan calendar. Earliest constraints raise the start.
- **End:** the day after the last assignment day. A task with no workers works `duration` days of
  the plan calendar, and affinity days still apply.
- **Affinities:** a task works only on days every linked affinity allows (`Plan::task_allows_day`).
  A person must also be available that day. Days an affinity disallows behave like calendar gaps,
  not blocks: they do not restart a strict run (**Assumed**). A task that can never be placed (no allowed days,
  not enough capacity within the horizon) is unscheduled with `NoCapacity`.
- **`ScheduleMode`** (`hours_on(wanted, available)`):
  - Strict (default): a day counts only if the full daily block is free for the worker. Strict
    tasks run **consecutively**: on a working day where the block cannot be claimed (someone else
    holds hours) the run restarts from the next day, undoing hours already taken. Calendar gaps
    (the person has 0 hours that day) do not break the run. With several workers all of them must
    have their full block on the same days, in step, else the run restarts. A day where someone has
    fewer hours than the block is a calendar gap too, and if anyone has a gap the day is skipped for
    all of them without a restart (**Assumed**).
  - Relaxed: any day with free time, using what is free up to the block, until the effort is done.
    It can take longer than `duration` and may skip days.
- **Time constraints** (against the start): Earliest raises the start, never violated. Latest and
  Fixed are violated when dependencies push the start past the date, then the item is placed at
  the earliest start and a `ConstraintViolation` records the constraint and landing date. Fixed
  starts exactly on its date when dependencies allow. After applying worker availability, re-check
  the constraint against the real start, and overwrite any earlier violation with the accurate date.

### Statuses

- **NotStarted:** scheduled normally.
- **InProgress:** claims its remaining capacity before any not-started task. Its start is the
  actual start date regardless of dependencies. The daily hours are applied from the start on the
  person's available days, and always at least up to today even past `duration` (so a task assigned
  5 days that took 7 shows 7 days). Its range ends no earlier than the day after today. Dependents
  are not scheduled from a past date.
- **OnHold:** ends on the day it was put on hold (`actual_end_date`, set by the update). No
  allocation is applied after that. For computing milestone or group derived status it counts as
  not completed. Dependents use the hold date as its end. Resuming (in progress) applies the
  daily hours again.
- Actual start and end dates are inclusive days (the first and last day worked), so a range ends the
  day after `actual_end_date` (**Assumed**). This holds for completed and on-hold tasks.
- **Completed:** fixed at its actual dates. Assignments for past days are inferred (below).
- **Dropped:** unscheduled with reason `Dropped`, transparent to dependents.
- Past work is not saved: for completed tasks and the past days of in-progress and on-hold tasks,
  apply the daily hours from the actual start forward on the assignee's available days. Each
  requested worker uses the person in `Task.actual_workers` when there is one, otherwise the named
  user, or for tags and groups the scheduler's resolution (section 4, Recorded workers).

### Milestones

- No user status, only a derived one. A zero-time event that lands on the last day of all its
  dependencies, including other milestones (see date conventions). It does not move to a working day:
  if someone works a Saturday to finish a dependency, the milestone lands on that Saturday, and a
  task after it could start Sunday, which in practice means the first day anyone works. It is
  re-checked against its constraint on its landing date.
- Derived status: complete when every dependency is done, in progress when any task it depends on
  is in progress, otherwise not started. Done means: a task Completed or Dropped;
  a milestone derived complete; a task group whose own dependencies and all members are done; a
  buffer seen through to its own dependencies (a milestone depending on a buffer sees the buffer's
  dependencies). In progress comes from tasks only, directly or through a buffer or group, and an
  in-progress milestone dependency is ignored.

### Task groups

One node around the items whose `parent` is the group (tasks, milestones, buffers, nested groups).
Its own dependencies decide the day its members can start: members begin at `ParentStart`, which
is that date, and so do members with no dependencies. The group then
covers the first member's start to the last member's end (`TaskGroup::place`), so it begins
overlapping its first task and its end falls on the last day a member runs. Milestone members count
as their one day. With no members the group lasts 3 calendar days from that date (not configurable); that stops
applying as soon as a member is added. A group whose members are all unscheduled is
unscheduled. External items depend on the group as a whole, on its end. A group with no
dependencies never starts, so it and its members are unscheduled. Groups nest. They have no time
constraints and no affinities.

### Recorded workers

`Task.actual_workers` maps a requested worker to the person who actually does the work (`Assignee`,
which may be a different person from a named user, or the placeholder). It is persistent state: it
fixes history so past hours do not move when availability or other plans change.

- Empty to begin with. When a task goes in progress, on hold or completed with nothing recorded,
  `Tenant::fill_defaults` fills it with the people last scheduled for it (the stored schedule's
  `workers` for the task). It runs in the server handler beside `assign_ids`, so the filled values
  are part of the logged update and applying an update never reads volatile state. With no stored
  schedule nothing is filled.
- Changeable with the task patch (`actual_workers`, replace the whole map, an empty map clears it).
- Only workers the task asks for can have an entry, dropping a worker with a recorded person fails.
  A recorded user must exist in the tenant, and a user recorded on any task cannot be removed from
  the tenant or the plan.
- The scheduler uses a recorded person for that worker (past days, and an in progress task keeps
  them going forward). Without an entry it resolves the worker itself, first finisher for a tag or
  group.

### Plan buffers

`PlanBuffer::place(start, calendar)` where `start` is when the buffer's dependencies finish and
`calendar` is the plan calendar. The duration is working days, the end is exclusive. Unset
(`set_start_day` is `None`): ends a duration after `start`. Set: ends where it would have if it
had started on `set_start_day`, so it shrinks when `start` is later and grows when earlier, never
below zero length. A task or milestone that cannot move later (Fixed or Latest constraint, date `X`)
and depends on a buffer shrinks the buffer to fit, set or unset: the buffer ends no later than
`limit = X` for a task (`X + 1` for a milestone), the smallest limit over such
dependents, so its end becomes `max(start, min(natural end, limit))` (**Assumed**). Only when the
buffer is already zero length and `start` is past the limit is the constraint violated. That is
the `limit` argument of `PlanBuffer::place`.

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

## 5. Unscheduled items

Every item is placed unless it falls under a reason below. An unscheduled dependency does not stop
an item that has another dependency that is placed (it is ignored when computing its start).
Reasons:

- `NoEligibleWorkers`: a tag or group resolves to nobody.
- `NoCapacity`: the workers exist but cannot provide the allocation (availability, affinities, or
  the horizon). An allocation shortfall counts as unscheduled.
- `Dropped`.
- `DependsOnUnscheduled`: every dependency was left out, by a failure rather than by having no path.
- `NoScheduledMembers`: a task group none of whose members could be scheduled.

## 6. Cross-plan model

Imagine every plan's nodes in one super-plan. Each plan has a start, one or more **targets**, and
shares people through one capacity ledger of remaining hours per person per day. A **target** is a
node the scheduler brings as far left as it can. It has no due date: its date is a forecast of when
the plan finishes if it runs as scheduled. Hard dates are separate `Fixed` and `Latest` constraints
on nodes. A plan with no explicit target uses its whole end as the target (every node is on the
chain).

### Baselines

Run each plan alone (only its tasks, the users' full calendars, no other plans) with Plinko 1's
stages. This gives:

- the plan-alone forecast of each target (the baseline, `I`),
- a resource-feasible solo schedule,
- the solo slack of each node, from a backward pass over dependency edges plus resource edges
  (task X precedes task Y on the same person in the solo schedule), so float is realistic.

A solo run is a `Baseline { state, slack }`, slack in calendar days (negative once a hard date is
missed). Slack works back from each target's solo end (or the plan end with none) and from hard
dates, over dependency edges and the order each person works their tasks in (a task follows the
previous one when their days do not overlap). Nodes feeding no target with no hard date have none.
Baselines are cached on the tenant by `scheduler::prepare`, dropped for a plan when it is updated
and for all plans on any other update (people, availability, the clock). `solve_tenant` sets each
forecast's baseline from them. Delay is the combined forecast minus the baseline, and is the metric for everything
below.

## 7. Algorithm

### Phase 0: anchors

Pre-insert every non-not-started task as fixed: completed, on hold, dropped. Deduct their future
hours from capacity (not dropped ones). Past work does not use future capacity.

### Phase 1: in progress

In-progress tasks of every plan claim their remaining capacity first, in dependency order. This
holds even when another plan's chain is longer. Their start comes from their actual start, not
their dependencies.

### Phase 2: baselines

Solo runs per plan as above.

### Phase 3: combined insertion

Insert all remaining nodes into one ledger, in strict layers:

1. **Hard tier:** every ancestor of a Fixed or Latest node that is urgent (see repair rounds, all of
   them to begin with), at any distance, as Plinko 1 did. A
   node's key is the latest day it could start to keep the nearest date it leads to: the date less
   the days of work from the node to the dated one (`Network::remaining`, a difference of the
   longest chains of work that follow each). The earliest key goes first, then the longest chain of
   work to follow, then plan order. Dates met by a wide margin are not special: only the order
   among dated chains and ahead of other work matters, and a task still goes as early as its
   dependencies and people allow.
2. **Target chain:** every node that is an ancestor of some plan's explicit targets, or of the plan
   end for a plan with none.
3. **Rest:** nodes feeding no explicit target. They are placed after every chain node, so they can
   only use hours the targets cannot, which keeps them from restricting a target, while still
   filling earlier gaps where it costs nothing. The layers apply across plans.

On top of that, a ready task with a hard date is taken ahead of whatever else is next when that task
would run past the date (the day the dated task must start by), since a person busy until then
could not start it in time (`PlanSolver::pressed`). With several, the earliest date goes first.

Within a layer, repeatedly take the most urgent node among those whose dependencies are all
placed. Plain slack ordering is not topological (a predecessor can have more slack than its
successor), so never sort the whole list by slack.

The **slip** of a node not yet placed is how many days later than in the plan-alone schedule its
dependencies now let it start, carried on to what follows as a shift of its solo range. Placed work
counts at its real dates, so a plan that lost contention shows it in everything after.

Urgency of a node is the maximum over the targets it feeds of `slip - solo slack to that target +
plan priority + target priority` (`Baseline::by_target`). This is the spec's "earliest finish of
the target through the node minus its baseline" without simulating placements, which would cost a
search per candidate per step. It is recomputed after every placement in the plan. Ties: the node
feeding more targets, then plan order (the latest solo end, then earlier start date, then plan id),
then lowest id. The rest layer is ordered by higher plan priority, then least solo slack against
the plan end (`Baseline::plan_slack`).
Impact is not spread within a plan yet: a task is only urgent once its own dependencies are late, so
one plan can take a contended person's hours until it is done. Step 7 spreads it.

Insertion mechanics (Plinko 1): earliest start from dependencies, apply Earliest,
resolve workers, fill the ledger per worker (strict consecutive or relaxed), set the range.
Constraint violations are read off the final placement. A node goes in only once everything it
depends on is settled (placed or left out), so a later insertion never moves one already placed.
Plinko 1's propagation and protected target are not needed (**Assumed**). Nodes that follow from
their dependencies alone (milestones, groups, buffers, dropped tasks) are placed as soon as they
are ready, and recomputed after compaction moves a task.

Choosing a person for a tag or group slot: the eligible person who finishes first (simulate the fill
without taking hours). Among candidates, accept one finishing up to one day later if their load in
the task's window is much lower, to avoid piling on a bottleneck (**Assumed** parameters: tolerance
1 day, and the candidate's booked hours at most half the best finisher's; tune with tests). Remaining
ties go to the lowest id. A task keeps its person for its whole run. In the combined run an equal
finish goes to the person the plan chose on its own, for stability, and a strictly earlier finish
for another person wins.

### Phase 4: compaction

Plinko 1's pass, across all plans: re-insert each not started task from its earliest start from
dependencies and keep the result only if it ends earlier. Chains to hard dates and targets go
first, earliest start first (ties by plan order), until nothing moves, then the rest, so loose work
never takes room a chain could still use, and again until stable, at most 50 passes. No
propagation inside a pass. Inserting by eligibility never leaves a gap that something placed
later could not already fill (random plans found no move), so this only does work after repair
rounds re-insert nodes.

### Phase 5: repair rounds (contention)

Plans will often be in contention for people, so spread the impact:

- **Objective:** lexicographic. First minimise the days hard dates are missed by in all, then how
  many are missed, then the worst effective delay across all targets (delay plus priority offsets),
  then among equal worst cases the total effective delay.
- **Start:** repair runs twice and keeps the better, once from every hard dated chain going first
  (the hard tier of phase 3) and once from none going first, as each finds results the other
  does not. Only chains in a plan's `urgent` set go first.
- **Round:** these changes are tried, each by scheduling everything again, and the best that
  beats the objective is kept (`scheduler/repair.rs`):
  1. Every worst target that is late (at the worst effective delay) gets a bump added to its
     urgency of as many days as it is late, adding up over rounds. The bump only applies within
     the layers, a chain in the hard tier cannot be overtaken.
  2. Tag and group work that is not started, in any plan, moves off the people who are bottlenecks
     (booked 0.9 or more of their time, `Ledger::load`) on the worst targets' chains, to the
     eligible person with the lowest load, if lower than theirs (`PlanSolver::repair`).
  3. Every hard date that was missed becomes urgent.
  4. Urgent hard dates that were kept with 7 or more days to spare stop being urgent.
  5. Hard dates are judged to need starting earlier by as many days as they were missed by, and
     later by half the room the kept ones had (`PlanSolver::tightening`), so the work that kept
     them gives way to the work that missed.
  Only the effective delay of targets counts, with the plan and target priority, not the bumps.
- **Bounds:** a fixed round cap (**Assumed**: 24), and stop when nothing improves. Deterministic,
  because bumps and candidate order depend only on state and ids.
- **Speed:** each round schedules from scratch. Checkpointing the ledger and state at each
  insertion step so a round restarts from the first step that changes is not done.
- **Spreading:** when choosing a person for a tag or group slot, someone finishing up to a day
  after the first to finish is chosen instead if their booked hours in the task's window are at
  most half (and less than) the first's (`SPREAD_DAYS`), for every placement.

### Finish

Build each `PlanVolatileState`, derive milestone and group values, then
`TenantVolatileState::from_plans`.

### Ordering work with no hard dates

A plan with no hard dates orders what feeds no target by the soonest it has to start: where it
started when the plan was alone plus its slack. Slack alone made the last task in the solo order
look critical and put it ahead of work others waited on. On `NODATES=1 rich` the plan ends came
out 0 to 4 % sooner and target delay down in most sets. Plans with hard dates keep ordering by
slack, where the same change made missed dates slightly worse.

### Dates that cannot be met

Each baseline records the earliest every hard dated node could start however free people are
(`Baseline::earliest`, `slack::early_starts`): the days of the work before it on the days the
people it names work, with tags and groups taken as the plain length. A miss no later than that is
unavoidable (`ConstraintViolation::avoidable`), so repair rounds do not count it, make it urgent or
pull work earlier for it, and the client says it cannot be met.

### Incremental updates

`scheduler::Tracker` applies updates and notes what each can change (`impact.rs`): nothing (names,
descriptions, data, favourites) leaves the schedule as is; task, milestone, group and buffer edits
name their nodes; tags, affinities, users, targets, availability and scheduler data mark the plan
whole; people, calendars and the day schedule everything. `incremental.rs` then places again only
the named nodes and what depends on them, with the other tasks kept where the last schedule had
them (`PlanSolver::claim_frozen`, skipped if their hours are taken), and the baselines and repair
changes (`Fixes`) kept from the last full run. If that is worse than before, everything starting
after the first changed task is placed again too and the better is kept. If it is still much worse
than the last full run (`DRIFT`), everything is scheduled again. The server worker (`net/worker.rs`) schedules everything again on a copy once
updates pause, and swaps it in if nothing changed meanwhile. `INCREMENTAL=n rich ...` measures it.

## 8. Priorities

- `Plan.priority: i32`, positive or negative, user set, default 0.
- Each target has `priority: i32`, default 0.
- Effective priority is the sum, used as an offset in days of lateness: +3 treats a plan as 3 days
  further behind than it is. Large values act like strict classes.
- Priority only matters on contention. It never reorders layers (hard tier, target chain, rest).

## 9. Impact and revert

No preview: a run is too costly to do per edit. Edits are applied, scheduled, and reverted if the
result is bad. Which plans moved and by how many days is a diff of the new schedule against the
one before it, a pure function to add when the UI wants it (the previous schedule must be kept,
`apply_update` clears it). Revert belongs to the update log, see `client.md`.

## 10. Data model work needed

1. **Targets** (done): `Plan::targets()`, `Target { id, node, priority }` with `Crud`, validated,
   blocking removal of the node. No targets means the whole plan end.
2. **Plan priority** (done): `PlanSchedulerData::priority()` with its patch.
3. **Recorded workers** (done): `Task.actual_workers`, `TaskPatch.actual_workers`,
   `task::VolatileState.workers`, `Tenant::fill_defaults`. See section 4.
4. **Actual dates** (done): a task that starts, goes on hold or completes without an
   `actual_start_date` begins today, and one completed without an end ends today
   (`Tenant::set_status_dates`).
5. **Output** (done): `TargetForecast` and `UserSchedule::bottleneck`, see section 3.
6. **Tenant clock** (done): `Tenant::today()`, `TenantUpdateType::SetToday`. Putting a task on hold
   uses it for the end date. Task duration and group placeholder default to 1 and cannot be 0.
7. **Buffer shrink for hard-constrained dependents** (done): `limit` argument on
   `PlanBuffer::place`, the latest end the buffer may take.
8. **Accessors the scheduler needs:** `Task::duration`, `mode` and `time_constraint` exist now.
   Check the other entities as the engine is written.

## 11. Code layout

- `scheduler/mod.rs`: `solve_tenant` and the phases.
- `scheduler/state.rs`: output types (exists).
- `scheduler/ledger.rs`: capacity ledger (exists).
- `scheduler/fill.rs`: strict and relaxed placement of slots against `Hours` (exists).
- `scheduler/network.rs`: dependency graph order and remaining work (exists). `status.rs`:
  derived milestone statuses (exists).
- `scheduler/slack.rs`: late ends and slack from the dependency and resource edges (exists).
- `scheduler/plan_solver.rs`: stages, task placement, worker choice, compaction (exists).
- `scheduler/combined.rs`: layers, urgency and the order of placing across plans (exists).
- `scheduler/repair.rs`: rounds of repair (exists).
- Reuse `availability::calendar::Calendar`, `Plan::unscheduled`, `Plan::task_allows_day`,
  `PlanBuffer::place`, `ScheduleMode::hours_on`, `TimeConstraint::apply`, `Dependency::start_after`.

## 12. Tests

Port Plinko 1's scheduler scenarios, then add:

- Two independent strict tasks sharing a person at 4h/day share a day, not serialised.
- Strict multi-worker tasks stay in step, and the lighter worker is spread over the whole span.
- A strict task restarts when another task holds part of a working day, but not across weekends.
- In-progress claims capacity before a longer not-started chain, and its range reaches today.
- A target milestone lands after all its predecessors.
- A dropped middle task is transparent; an on-hold task ends at its hold date, not complete.
- A delay holds back what follows it by calendar days.
- Fixed and Latest constraints, violations, and the recheck against the real start.
- A milestone lands on a non-working day when a dependency finishes on one, and a task after it
  starts on the first day anyone works.
- Constraint chains outrank target chains, which outrank the rest.
- A rest task fills an early gap but never delays a target's forecast.
- Two plans contending for one person end with delays spread by priority; a plan priority offset
  shifts the outcome; repair rounds improve the worst delay.
- A tag or group slot picks the first finisher, and a lightly loaded alternative within the
  tolerance.
- Affinity-restricted tasks use only allowed days, and a task with none is unscheduled.
- Determinism: shuffling insertion order of map entries gives identical results.
- Plan-alone baselines are lower bounds checked against combined forecasts.

## 13. Build order

Each step is testable on its own before the next starts. Steps 1 to 3 give a working single-plan
scheduler early.

1. **Prerequisites** (done).
   - Targets: replace `PlanSchedulerData.scheduler_target` with a list of `{ id, node, priority }`
     with `Crud`, validation, and in-use blocking of the node.
   - `Plan.priority: i32` with its patch.
   - Volatile outputs: per-target forecast and delay, per-person bottleneck load.
   - `PlanBuffer::place` takes an optional `limit` for the shrink rule.
   - Check the other entities for the accessors the engine needs.
2. **Capacity ledger** (done, `scheduler/ledger.rs`). Remaining hours per person per day as arrays over the horizon, built from
   `Tenant::refresh_calendars` and the plan calendar. A `Run` of claims is
   taken atomically (`Ledger::claim`) and given back (`release`), `Clone` checkpoints it. Claims on
   days before the window are ignored, so anchors and in-progress tasks deduct only future hours.
   `load` gives the bottleneck share.
3. **Single-plan insertion (port of Plinko 1)** (done, `scheduler/plan_solver.rs`). For one plan alone: anchors, in-progress, constrained
   nodes, target chain, rest, in that order, with CPM ordering. Earliest start from dependencies
   (the date conventions in section 3), strict (consecutive, synchronised) and relaxed fills,
   worker resolution by first finisher, constraints and violations, propagation and re-insertion,
   compaction. Milestone, group and buffer dates and derived milestone status. Unscheduled reasons.
   Tests: Plinko 1's scenarios (section 12).
4. **Baselines and solo slack** (done, `scheduler/slack.rs`). Run step 3 per plan alone, record each target's forecast, and
   compute solo slack on dependency plus resource edges. Cache per plan with invalidation when the
   plan or its people's availability changes.
5. **Combined insertion** (done, `scheduler/combined.rs`). One shared ledger. The hard tier, target-chain and rest layers across
   plans, the eligibility queue, dynamic urgency with priority offsets, and the plan-alone solo
   assignments as defaults. Tests: contention, priority, a rest task never delaying a target.
6. **Compaction** across all plans (done, `combined::compact`).
7. **Repair rounds** (done, `scheduler/repair.rs`). The objective (worst effective delay, then total), temporary priority bumps,
   alternative assignees for bottleneck people, checkpointing, and the round cap. Then the spreading
   rule for tag and group slots.
8. **Outputs and integration** (done). Forecasts, delays, bottleneck loads and `from_plans` come from
   the solver. `Tenant::stage` (fresh ids, then `fill_defaults`) builds the update to log,
   `Tenant::apply_and_schedule` applies it and runs `scheduler::schedule` (`prepare`, `solve_tenant`,
   `store_schedule`). 

At each step keep the determinism test (shuffled map order gives identical results) green.

## Performance

`cargo run --release --example bench` schedules 5 plans of 200 tasks with 20 people: 80 ms to
prepare, 0.9 s to solve (a run of about 170 ms, repair rounds run their two changes on separate
threads), and a 324 KiB schedule on the wire. Most of a run is working out slips for plans with hard
dates, which could be made incremental.

## Open items and assumptions

- Slack and the hard tier margin (3) are in calendar days. Spread tolerance (1 day, half the load), repair round cap (8).
- Unstarted tasks start no sooner than tomorrow (Plinko 1).
- Strict days with fewer hours than the block are gaps, anyone having a gap skips the day for all.
- A task starts on the first day work is placed, not on the plan calendar's first working day.
- Unscheduled dependencies are ignored, dropped ones transparent (Plinko 1).
- Dependents of an on-hold task use its hold date as the end.
- Defaulting actual dates on in progress and completed.
- A task started with no stored schedule (nothing to record) resolves its tag and group workers itself until someone records them.

## Moving between groups

`Plan::moving(node, parent)` gives the updates. Entering a group: what the item waited on moves onto the group (except what is inside it), what waited on the item from outside waits on the group's end, and the item waits on the group's start. Leaving: the item's group-start dependency is replaced by the group's own dependencies. A group cannot move into itself.

## Tried and not kept

- **Holding hours backward from hard dates.** Before placing anything, schedule the chains to hard
  dates backward (latest dates first, each node as late as its date and the hours held for others
  allow), hold those hours, and give each node's back when it is placed. A node can then always
  use its held slot, so a chain not ready yet is not crowded out. With the made up plans of
  `examples/score.rs` this was much worse than not holding (about 3 to 10 times the misses). The
  greedy backward schedule often cannot fit a node near the start (its dependents were held
  too early by hours other nodes held), which makes the held slots of everything after it
  wrong, and dropping those leaves most of the chain unheld; what is still held takes hours the
  rest could use. Using the backward start times only to order work (no hours held) was within
  noise of the present ordering, a little better on one and two plans, worse on three.

## How close to optimal

`examples/rich.rs` plans solved exactly with CP-SAT (`examples/cpsat.py`, 8 cores) and accepted by the
scheduler as valid (every task placed on the solver's day with its person, nothing unscheduled):

- Eight plans of 500 tasks and 12 people: CP-SAT proved the optimum in 9 to 16 s each. It kept every
  date (the scheduler missed 14 in all, 77 days late) and the plan was the same length or up to
  11 % shorter (2.7 % on average).
- One plan of 1000 tasks and 40 people with half days, affinities, tags and groups: all dates kept
  (the scheduler missed 7, 46 days late), the plan to its target 3.5 % shorter and the average task
  finishing 2 % sooner, though the last task not feeding the target finished later.
- Four such plans of 1000 tasks sharing 40 people (4000 tasks): CP-SAT improved on the scheduler
  only to 13 missed dates and 67 days late (the scheduler 17 and 101) in 46 minutes of
  neighbourhood search, a plan end of 683 days against 687, and ran out of the memory of the
  8 GB machine; it is not near optimal at that size. The scheduler took 29 to 54 s there.
- Four plans of 250 tasks sharing 20 people (three seeds, 15 minutes of CP-SAT on 8 cores each, all
  accepted by the scheduler as valid): CP-SAT kept every date in two and missed the same two days as
  the scheduler in the third, with the plan to its target 1 to 6 % shorter, the last task 1 to 10 %
  sooner and the average task finishing 6 to 11 % sooner. The scheduler took about a second.

Against the scheduler from before hard dates went first (`rich`, averages): four plans of 250 tasks
and 20 people missed 22 dates by 684 days, now 1 by 4; the same with another seed 18.5 by 626, now
1.5 by 3; one plan of 1000 tasks and 40 people 8.3 by 101, now 2.7 by 16, with plan ends within 5 %
(shorter now). Incremental updates (`INCREMENTAL=n rich`) end at the same misses as a fresh run
(four plans of 1000 tasks: 1.9 s mean per edit against 10 s, 250 tasks: 0.06 to 0.3 s against 0.6 to
2.5 s), with the occasional edit needing a full run.

