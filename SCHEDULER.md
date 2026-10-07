# Scheduler

Specification for `scheduler::solve_tenant(&Tenant) -> TenantVolatileState`. It schedules every plan
in a tenant together, because people work across plans. Plinko 1's scheduler
(`plinko/plinko-shared/src/data/scheduler.rs`) is the reference for the engine. This document
records what is settled and what carries over from it, and lists the changes still needed in the
data model.

Status: the output state, availability calendars, affinities, groups, recorded workers, buffers,
`ScheduleMode`, `TimeConstraint::apply`, the ledger and the single-plan solver exist. Plans are
scheduled one after another into the shared ledger, the plan that ends last when alone first, then
the earlier start date, then the lower id, which the combined insertion of step 5 replaces. Call `scheduler::prepare` to cache calendars, then `solve_tenant`.
Items marked **Assumed** were not explicitly confirmed and are collected at the end.

## 1. Principles

- The scheduler is a pure function of tenant state. The same updates always produce the same dates.
  Memory layout and `HashMap` order do not matter, so results must not depend on iteration order.
  Break every tie by id (v7 ids order by creation).
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
  (`start_date`), targets and priorities (see section 10).
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
- Dependency `lag` is calendar days of the plan calendar, negative for lead.
- Time constraint dates are the start of the task or milestone (`TimeConstraint::apply(earliest)`).
- Unstarted tasks cannot start before tomorrow (Plinko 1; **Assumed**). Milestones may land on
  today. A milestone whose derived status is complete may land in the past (it already happened).
- Plan start (`ParentStart` at top level) is `max(today, plan start_date)`; a derived-complete
  milestone uses the plan's `start_date`.
- Every node has an exclusive end, the day after its last day. A milestone occupies the one day it
  lands on (`milestone::VolatileState::end()`). A task, group or buffer starts at a dependency's
  end plus lag (`Dependency::start_after`), so a task after a milestone starts the day after it.
  A milestone lands on the last day of everything it depends on, tasks, groups, buffers and other
  milestones alike, plus lag (`Dependency::milestone_date_after`): milestones are zero time events
  at the trailing edge of their real work, and share a day with milestones they depend on.
  `ParentStart` is a date, not a node, so `start_after` applies to it for milestones as well.
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
  is in progress, otherwise not started. Done means: the plan start; a task Completed or Dropped;
  a milestone derived complete; a task group whose own dependencies and all members are done; a
  buffer seen through to its own dependencies (a milestone depending on a buffer sees the buffer's
  dependencies). In progress comes from tasks only, directly or through a buffer or group, and an
  in-progress milestone dependency is ignored.

### Task groups

One node around the items whose `parent` is the group (tasks, milestones, buffers, nested groups).
Its own dependencies decide the day its members can start: members anchor with `ParentStart`, which
is that date. A top-level group needs an explicit `ParentStart` dependency to be anchored to the
plan start, exactly like a task. The group then
covers the first member's start to the last member's end (`TaskGroup::place`), so it begins
overlapping its first task and its end falls on the last day a member runs. Milestone members count
as their one day. With no members the group lasts `placeholder_duration` working days of the plan
calendar (default 1, at least 1) from that date; the placeholder stops applying as soon as a member is added (the value is
kept, so it applies again if the group empties). A group whose members are all unscheduled is
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
`limit = X - lag` for a task (`X - lag + 1` for a milestone), the smallest limit over such
dependents, so its end becomes `max(start, min(natural end, limit))` (**Assumed**). Only when the
buffer is already zero length and `start` is past the limit is the constraint violated. That is
the `limit` argument of `PlanBuffer::place`.

## 5. Unscheduled items

`Plan::unscheduled()` already finds items with no dependency chain to the plan start. Any chain
reaching the plan start is enough. An unscheduled dependency does not stop an item that has
another anchored path (the unscheduled dependency is ignored when computing its start). Reasons:

- `NoPathToPlanStart`: no dependency chain reaches the plan start.
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

1. **Hard tier:** ancestors of Fixed or Latest constrained nodes whose slack to the hard date is at
   or below a margin (violated ones too). Order by hard slack ascending (most violated first),
   ties by earlier date. Chains with more slack compete in the target layer until their slack falls
   to the margin (**Assumed** margin: 3 working days, a tunable constant).
2. **Target chain:** every node that is an ancestor of some plan's explicit targets.
3. **Rest:** nodes feeding no explicit target. They are placed after every chain node, so they can
   only use hours the targets cannot, which keeps them from restricting a target, while still
   filling earlier gaps where it costs nothing. The layers apply across plans.

Within a layer, repeatedly take the most urgent node among those whose dependencies are all
placed. Plain slack ordering is not topological (a predecessor can have more slack than its
successor), so never sort the whole list by slack.

Urgency of a node is the maximum over the targets it feeds of `proj + plan priority + target
priority` where `proj = (earliest feasible finish of the target through this node, from the current
ledger) - baseline forecast`. It is recomputed at every step, so a plan that keeps losing
contention rises and the impact spreads. Ties: the node feeding more targets, then lowest id. The
rest layer is ordered by plan priority, then its plan-alone whole-plan urgency.
Remaining work to a target uses the longest dependency path of planned durations plus non-negative
lags (milestones 0, buffers their duration).

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
ties go to the lowest id. A task keeps its person for its whole run. Solo choices are the default in
the combined run and change only when the person is overbooked, for stability.

### Phase 4: compaction

Plinko 1's pass: for every not-started task or milestone (earliest start first), re-insert it from
its earliest start from dependencies and keep the result only if it ends earlier (or fills gaps).
No propagation inside a pass. Repeat until nothing improves, at most 50 passes.

### Phase 5: repair rounds (contention)

Plans will often be in contention for people, so spread the impact:

- **Objective:** lexicographic. First minimise the worst effective delay across all targets
  (delay plus priority offsets), then among equal worst cases the total effective delay.
- **Round:** take the worst-hit plans, add a temporary priority bump to their targets, and
  re-insert. Also try giving tasks on their chain to a different eligible person when the current
  person is a bottleneck (heavily booked by other plans' chain work). Keep the new schedule only
  if the objective improves.
- **Bounds:** a fixed round cap (**Assumed**: 8), and stop when nothing improves. Deterministic,
  because bumps and candidate order depend only on state and ids.
- **Speed:** checkpoint the ledger and state at each insertion step so a round restarts from the
  first step that changes.

### Finish

Build each `PlanVolatileState`, derive milestone and group values, then
`TenantVolatileState::from_plans`.

## 8. Priorities

- `Plan.priority: i32`, positive or negative, user set, default 0.
- Each target has `priority: i32`, default 0.
- Effective priority is the sum, used as an offset in days of lateness: +3 treats a plan as 3 days
  further behind than it is. Large values act like strict classes.
- Priority only matters on contention. It never reorders layers (hard tier, target chain, rest).

## 9. Impact preview (later)

Because results are a pure function of tenant state, "state + update" against "state" shows which
plans move and by how many days. Not part of the first solver.

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
- Planned: combined insertion, and repair rounds, in separate files.
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
- Negative lag starts a dependent before its predecessor ends.
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
5. **Combined insertion.** One shared ledger. The hard tier, target-chain and rest layers across
   plans, the eligibility queue, dynamic urgency with priority offsets, and the plan-alone solo
   assignments as defaults. Tests: contention, priority, a rest task never delaying a target.
6. **Compaction** across all plans.
7. **Repair rounds.** The objective (worst effective delay, then total), temporary priority bumps,
   alternative assignees for bottleneck people, checkpointing, and the round cap. Then the spreading
   rule for tag and group slots.
8. **Outputs and integration.** `TenantVolatileState::from_plans`, forecasts, delays and
   bottleneck loads, the runner calling `solve_tenant` then `Tenant::store_schedule` after every
   update and `Tenant::fill_defaults` in the handler. Impact preview afterwards (section 9).

At each step keep the determinism test (shuffled map order gives identical results) green.

## Open items and assumptions

- Slack is in calendar days, so the hard tier margin is 3 calendar days until step 5 says otherwise.
- Hard tier margin (3 working days), spread tolerance (1 day, half the load), repair round cap (8).
- Unstarted tasks start no sooner than tomorrow (Plinko 1).
- Strict days with fewer hours than the block are gaps, anyone having a gap skips the day for all.
- A task starts on the first day work is placed, not on the plan calendar's first working day.
- Unscheduled dependencies are ignored, dropped ones transparent (Plinko 1).
- Dependents of an on-hold task use its hold date as the end.
- Defaulting actual dates on in progress and completed.
- A task started with no stored schedule (nothing to record) resolves its tag and group workers itself until someone records them.
