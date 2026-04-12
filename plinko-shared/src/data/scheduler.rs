use crate::data::allocation::{
    ConstraintViolation, MilestoneAllocation, NodeAllocations, Status, TaskAllocation, TaskState,
    WorkSegment,
};
use crate::data::constraint::ConstraintKind;
use crate::data::ids::TagId;
use crate::data::task::WorkerSlot;
use crate::data::{MilestoneId, NodeId, Plan, TaskId, UserId, constraint};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

const EPSILON: f32 = 1e-6;
const MAX_FILL_DAYS: i64 = 3_650; // ~10 years

// SchedulerError {{{
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchedulerError {
    EmptyChain,
    MissingTaskAffinity {
        task_name: String,
        required_tags: HashSet<TagId>,
    },
    SpecificWorkerNotFound {
        task_name: String,
        user_id: UserId,
    },
    NoPathsToNode(NodeId),
    DisconnectedNode(NodeId),
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SchedulerError::EmptyChain => write!(f, "expected content in the node chain"),
            SchedulerError::MissingTaskAffinity {
                task_name,
                required_tags,
            } => {
                if required_tags.is_empty() {
                    write!(
                        f,
                        "task \"{task_name}\" has placeholder workers but no users are in the plan"
                    )
                } else {
                    let mut tags: Vec<String> =
                        required_tags.iter().map(|id| id.0.to_string()).collect();
                    tags.sort_unstable();
                    write!(
                        f,
                        "task \"{task_name}\" has no users with the required tags: {}",
                        tags.join(", ")
                    )
                }
            }
            SchedulerError::SpecificWorkerNotFound { task_name, user_id } => {
                write!(
                    f,
                    "task \"{task_name}\" references user {user_id:?} who is not in the plan"
                )
            }
            SchedulerError::NoPathsToNode(node_id) => {
                write!(f, "no path from plan start to node {node_id:?}")
            }
            SchedulerError::DisconnectedNode(node_id) => {
                write!(f, "node {node_id:?} has no path back to PlanStart")
            }
        }
    }
}
// }}}
// }}}

// SchedulerState {{{
struct SchedulerState {
    capacity: HashMap<(UserId, NaiveDate), f32>,
    allocations: NodeAllocations,
    inserted: HashSet<NodeId>,
    today: NaiveDate,
    /// InProgress tasks that have no future work segments and need to be
    /// scheduled dynamically from today rather than locked as anchored.
    inprogress_ids: HashSet<TaskId>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl SchedulerState {
    fn new(today: NaiveDate) -> Self {
        Self {
            capacity: HashMap::new(),
            allocations: NodeAllocations::default(),
            inserted: HashSet::new(),
            today,
            inprogress_ids: HashSet::new(),
        }
    }
}
// }}}
// }}}

// Scheduler Computation {{{
// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Plan {
    pub fn compute_time_optimised_plan(&mut self) -> Result<(), SchedulerError> {
        let today = chrono::Local::now().date_naive();

        // If a task has actual_start set but its status is NotStarted, the user
        // has reset it — clear actual_start to defer to the status.
        let not_started_ids: Vec<TaskId> = self
            .tasks
            .keys()
            .filter(|id| {
                self.node_allocations
                    .tasks
                    .get(id)
                    .map(|ts| ts.status == Status::NotStarted)
                    .unwrap_or(true)
            })
            .copied()
            .collect();
        for id in not_started_ids {
            if let Some(task) = self.tasks.get_mut(&id) {
                task.actual_start = None;
            }
        }

        // If a task has actual_end (corrected_end_date) set but its status is not Complete
        // or Dropped, clear it — e.g. task was reverted to InProgress.
        let non_terminal_ids: Vec<TaskId> = self
            .tasks
            .keys()
            .filter(|id| {
                self.node_allocations
                    .tasks
                    .get(id)
                    .map(|ts| !matches!(ts.status, Status::Complete | Status::Dropped))
                    .unwrap_or(true)
            })
            .copied()
            .collect();
        for id in non_terminal_ids {
            if let Some(ts) = self.node_allocations.tasks.get_mut(&id)
                && let TaskAllocation::Fixed {
                    corrected_end_date, ..
                } = &mut ts.allocation
            {
                *corrected_end_date = None;
            }
        }

        // Stretch any overrunning InProgress tasks
        let in_progress_ids: Vec<TaskId> = self
            .tasks
            .keys()
            .filter(|id| {
                self.node_allocations
                    .tasks
                    .get(id)
                    .map(|ts| ts.status == Status::InProgress)
                    .unwrap_or(false)
            })
            .copied()
            .collect();
        for id in in_progress_ids {
            let scheduled_end = {
                let task = &self.tasks[&id];
                match self
                    .node_allocations
                    .tasks
                    .get(&id)
                    .map(|ts| &ts.allocation)
                {
                    Some(TaskAllocation::Fixed {
                        end_date,
                        corrected_end_date,
                        ..
                    }) => corrected_end_date.unwrap_or(*end_date),
                    Some(TaskAllocation::Dynamic {
                        scheduled_end_date, ..
                    }) => *scheduled_end_date,
                    None => {
                        let start = today;
                        let d = task.effective_duration_days().ceil() as i64;
                        start + chrono::Duration::days(d.max(0))
                    }
                }
            };
            if scheduled_end < today {
                // Stretch the fixed allocation's end date to today
                if let Some(ts) = self.node_allocations.tasks.get_mut(&id) {
                    match &mut ts.allocation {
                        TaskAllocation::Fixed {
                            corrected_end_date, ..
                        } => {
                            *corrected_end_date = Some(today);
                        }
                        TaskAllocation::Dynamic { .. } => {
                            let start = ts.allocation.start_date();
                            ts.allocation = TaskAllocation::Fixed {
                                start_date: start,
                                end_date: today,
                                corrected_end_date: None,
                                time_allocation: vec![],
                            };
                        }
                    }
                }
            }
        }

        // Stage 1 – Validate
        self.all_tasks_completable()?;
        let dependents_map = self.build_dependents_map();
        self.check_all_nodes_connected(&dependents_map)?;
        let mut state = SchedulerState::new(today);
        self.pre_insert_anchored_tasks(&mut state);

        // Stage 1.5 – InProgress tasks first.
        // InProgress tasks have already started and must claim their remaining
        // future capacity before any NotStarted task is scheduled.  Without
        // this pass a long-chain NotStarted task sorted first by critical-path
        // length could consume the shared capacity an InProgress task needs,
        // pushing it further into the future.
        //
        // Use the same topological/critical-path order as later stages so that
        // if one InProgress task depends on another they are inserted correctly.
        // Because earliest_start_from_dependencies returns actual_start for
        // InProgress tasks regardless of deps, ordering does not affect start
        // dates — only the capacity reservation matters.
        let full_topo = self.get_priority_sorted_task_list_to_ends(&dependents_map)?;
        for id in &full_topo {
            if let NodeId::Task(tid) = id
                && state.inprogress_ids.contains(tid)
                && !state.inserted.contains(id)
            {
                self.insert_node(*id, &mut state, &dependents_map, None)?;
            }
        }

        // Stage 2 – Time-constrained nodes
        let time_constrained = self.get_time_constrained_nodes();
        for node in time_constrained {
            let list = self.get_priority_sorted_task_list_to_node(node, &dependents_map)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, None)?;
                }
            }
        }

        // Stage 3 – scheduler_target dependents
        // Guard: if the target node was deleted from the plan (e.g. a milestone that was
        // removed after being set as the target), fall back to PlanStart so the scheduler
        // doesn't error trying to path-find to a non-existent node.
        let target = match self.scheduler_target {
            NodeId::Task(tid) if !self.tasks.contains_key(&tid) => NodeId::PlanStart,
            NodeId::Milestone(mid) if !self.milestones.contains_key(&mid) => NodeId::PlanStart,
            other => other,
        };
        if !matches!(target, NodeId::PlanStart) {
            let list = self.get_priority_sorted_task_list_to_node(target, &dependents_map)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, Some(target))?;
                }
            }
        }

        // Stage 4 – Remaining end nodes
        let protect = if matches!(target, NodeId::PlanStart) {
            None
        } else {
            Some(target)
        };
        let list = self.get_priority_sorted_task_list_to_ends(&dependents_map)?;
        for id in list {
            if !state.inserted.contains(&id) {
                self.insert_node(id, &mut state, &dependents_map, protect)?;
            }
        }

        // Stage 5 – Compact: pull tasks back into gaps left by forward propagation.
        // Repeat until no task can be moved earlier.
        let mut changed = true;
        let mut iterations = 0;
        while changed && iterations < 50 {
            changed = self.compact_pass(&mut state)?;
            iterations += 1;
        }

        // Commit
        self.node_allocations = state.allocations;
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn insert_node(
        &self,
        node_id: NodeId,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        match node_id {
            NodeId::Task(tid) => self.insert_task(tid, state, dependents_map, protect_node),
            NodeId::Milestone(mid) => {
                self.insert_milestone(mid, state, dependents_map, protect_node)
            }
            NodeId::PlanStart => Ok(()),
        }
    }

    fn hours_remaining(&self, state: &mut SchedulerState, user_id: UserId, date: NaiveDate) -> f32 {
        *state
            .capacity
            .entry((user_id, date))
            .or_insert_with(|| self.hours_available(&user_id, date))
    }

    fn pre_insert_anchored_tasks(&self, state: &mut SchedulerState) {
        for &id in self.tasks.keys() {
            let ts = match self.node_allocations.tasks.get(&id) {
                Some(ts) if ts.status != Status::NotStarted => ts,
                _ => continue,
            };

            let (start, end, status, time_alloc) = match &ts.allocation {
                TaskAllocation::Fixed {
                    start_date,
                    end_date,
                    corrected_end_date,
                    time_allocation,
                } => (
                    *start_date,
                    corrected_end_date.unwrap_or(*end_date),
                    ts.status,
                    time_allocation.clone(),
                ),
                TaskAllocation::Dynamic {
                    scheduled_start_date,
                    scheduled_end_date,
                    time_allocation,
                } => (
                    *scheduled_start_date,
                    *scheduled_end_date,
                    ts.status,
                    time_allocation.clone(),
                ),
            };

            // InProgress tasks are always scheduled dynamically from their actual_start
            // date rather than being locked. They go into inprogress_ids so the scheduler
            // treats them as needing fresh allocation.
            if status == Status::InProgress {
                state.inprogress_ids.insert(id);
                // Don't pre-insert; insert_task will schedule from actual_start.
                continue;
            }

            // Dropped tasks are transparent: they contribute no duration and hold
            // no capacity, so dependents can start as soon as the dropped task's
            // own predecessors would have allowed.  We still record them in the
            // allocation map (at their historical start date) so dependency lookups
            // have a date to anchor to, but we skip the capacity deduction.
            let is_dropped = status == Status::Dropped;
            if !is_dropped {
                // Deduct future time allocation from capacity so subsequently scheduled
                // tasks cannot double-book hours already consumed by this anchored task.
                for seg in &time_alloc {
                    if seg.date >= state.today {
                        let avail = self.hours_available(&seg.user, seg.date);
                        let entry = state.capacity.entry((seg.user, seg.date)).or_insert(avail);
                        *entry = (*entry - seg.hours_worked).max(0.0);
                    }
                }
            }

            state.allocations.tasks.insert(
                id,
                TaskState {
                    status,
                    allocation: TaskAllocation::Fixed {
                        start_date: start,
                        end_date: end,
                        corrected_end_date: None,
                        time_allocation: time_alloc,
                    },
                },
            );
            state.inserted.insert(NodeId::Task(id));
        }
    }

    fn earliest_start_from_dependencies(
        &self,
        node_id: NodeId,
        state: &SchedulerState,
    ) -> NaiveDate {
        // InProgress tasks have already started — their actual_start is the
        // definitive start date regardless of dependencies.
        if let NodeId::Task(tid) = node_id
            && state.inprogress_ids.contains(&tid)
        {
            return self
                .tasks
                .get(&tid)
                .and_then(|t| t.actual_start)
                .or_else(|| {
                    self.node_allocations
                        .tasks
                        .get(&tid)
                        .and_then(|ts| match &ts.allocation {
                            TaskAllocation::Fixed { start_date, .. } => Some(*start_date),
                            _ => None,
                        })
                })
                .unwrap_or(state.today);
        }

        let deps = self.get_dependencies(&node_id);
        let is_milestone = matches!(node_id, NodeId::Milestone(_));

        // A derived-complete milestone (all predecessors are done) is allowed to
        // land in the past — it already happened. Non-complete milestones and all
        // tasks are constrained to today or later.
        let is_derived_complete = if let NodeId::Milestone(mid) = node_id {
            self.milestone_derived_complete(mid, state)
        } else {
            false
        };

        // Milestones are pure date markers — they sit where their dependencies land.
        // Tasks cannot start in the past; unstarted tasks start no sooner than tomorrow.
        let tomorrow = state.today + chrono::Duration::days(1);
        let mut earliest = if deps.is_empty() {
            if is_milestone {
                self.start_date
            } else {
                tomorrow.max(self.start_date)
            }
        } else {
            self.start_date
        };

        for dep in deps {
            // For derived-complete milestones, PlanStart is treated as self.start_date
            // (the milestone already happened). For everything else, PlanStart is
            // today.max(start_date) so nothing unfinished is scheduled in the past.
            let pred_end = if is_derived_complete && dep.id == NodeId::PlanStart {
                self.start_date
            } else {
                self.node_end_date_in_state(dep.id, state)
            };
            let lag = dep.lag_days.round() as i64;
            // For task predecessors: tasks need the *next* day to start work, so add 1.
            // Milestones are date-point markers: a milestone successor can land on the
            // same day as its predecessor (milestone or task). But a task that follows a
            // milestone must start the day *after* the milestone — the milestone date is
            // considered the "completion event", not a working day.
            let start_after = match dep.id {
                NodeId::PlanStart => pred_end + chrono::Duration::days(lag),
                NodeId::Milestone(_) => {
                    if is_milestone {
                        // milestone → milestone: overlap allowed, no +1
                        pred_end + chrono::Duration::days(lag)
                    } else {
                        // milestone → task: task starts the day after the milestone
                        pred_end + chrono::Duration::days(lag + 1)
                    }
                }
                NodeId::Task(tid) => {
                    // A dropped task is transparent — it has no duration and holds no
                    // capacity.  `pred_end` is already its start_date (the earliest it
                    // could have begun), so the dependent can begin on that same date
                    // without adding the usual +1 task-to-task offset.
                    let pred_dropped = state
                        .allocations
                        .tasks
                        .get(&tid)
                        .map(|ts| ts.status == Status::Dropped)
                        .unwrap_or(false);
                    if is_milestone || pred_dropped {
                        pred_end + chrono::Duration::days(lag)
                    } else {
                        pred_end + chrono::Duration::days(lag + 1)
                    }
                }
            };
            earliest = earliest.max(start_after);
        }

        // Apply Earliest constraint
        let earliest_constraint = match node_id {
            NodeId::Task(id) => self
                .tasks
                .get(&id)
                .and_then(|t| t.constraint)
                .filter(|c| c.kind == constraint::ConstraintKind::Earliest)
                .map(|c| c.date),
            NodeId::Milestone(id) => self
                .milestones
                .get(&id)
                .and_then(|m| m.constraint)
                .filter(|c| c.kind == constraint::ConstraintKind::Earliest)
                .map(|c| c.date),
            NodeId::PlanStart => None,
        };
        if let Some(ec) = earliest_constraint {
            earliest = earliest.max(ec);
        }

        // Tasks cannot start in the past; unstarted tasks start no sooner than tomorrow.
        if !is_milestone {
            earliest = earliest.max(tomorrow);
        }

        earliest
    }

    fn node_end_date_in_state(&self, node_id: NodeId, state: &SchedulerState) -> NaiveDate {
        match node_id {
            NodeId::PlanStart => state.today.max(self.start_date),
            NodeId::Milestone(mid) => state
                .allocations
                .milestones
                .get(&mid)
                .map(|a| a.date())
                .unwrap_or(self.start_date),
            NodeId::Task(tid) => state
                .allocations
                .tasks
                .get(&tid)
                .map(|ts| {
                    // Dropped tasks are transparent: they contribute no duration to
                    // their dependents.  Use start_date so the dependent can begin as
                    // soon as the dropped task's own predecessors would have allowed it.
                    if ts.status == Status::Dropped {
                        ts.allocation.start_date()
                    } else {
                        ts.allocation.end_date()
                    }
                })
                .unwrap_or(self.start_date),
        }
    }

    fn advance_working_days(&self, start: NaiveDate, count: u32) -> NaiveDate {
        if count == 0 {
            return start;
        }
        let mut current = start;
        let mut remaining = count;
        let limit = start + chrono::Duration::days(MAX_FILL_DAYS);
        while current <= limit {
            let hours = if let Some(h) = self.calendar.get(current) {
                h
            } else {
                let wd = crate::data::schedule::chrono_to_weekday(current.weekday());
                self.default_schedule.hours_on(wd)
            };
            if hours > 0.0 {
                remaining -= 1;
                if remaining == 0 {
                    return current;
                }
            }
            current += chrono::Duration::days(1);
        }
        current
    }

    /// Advances `date` forward until it lands on a day with plan-level capacity > 0.
    /// Returns `date` unchanged if it is already a working day.
    fn next_working_day_on_or_after(&self, date: NaiveDate) -> NaiveDate {
        let mut d = date;
        let limit = date + chrono::Duration::days(MAX_FILL_DAYS);
        while d <= limit {
            let hours = if let Some(h) = self.calendar.get(d) {
                h
            } else {
                let wd = crate::data::schedule::chrono_to_weekday(d.weekday());
                self.default_schedule.hours_on(wd)
            };
            if hours > 0.0 {
                return d;
            }
            d += chrono::Duration::days(1);
        }
        date
    }

    fn fill_slot(
        &self,
        user_id: UserId,
        total_hours: f32,
        start_date: NaiveDate,
        max_per_day: Option<f32>,
        strict: bool,
        state: &mut SchedulerState,
    ) -> Vec<WorkSegment> {
        let mut remaining = total_hours;
        let mut segments: Vec<WorkSegment> = Vec::new();
        let mut current = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining > EPSILON && current <= limit {
            let avail_full = self.hours_available(&user_id, current);
            let avail = self.hours_remaining(state, user_id, current);
            let scheduled = if strict {
                // In strict mode only schedule on days where the user has at
                // least cap hours of capacity remaining, so the task never
                // spreads its daily block across a partially-full day.
                // When no explicit cap is given, use the user's full daily hours
                // so that a multi-day task doesn't attempt to fit all remaining
                // work into one day (which is impossible and causes restart loops).
                let cap = max_per_day.unwrap_or(avail_full);
                if avail >= cap - EPSILON {
                    cap.min(remaining).min(avail)
                } else {
                    0.0
                }
            } else if avail > EPSILON {
                let cap = max_per_day.unwrap_or(f32::MAX);
                avail.min(remaining).min(cap)
            } else {
                0.0
            };
            if scheduled > EPSILON {
                let entry = state
                    .capacity
                    .entry((user_id, current))
                    .or_insert_with(|| self.hours_available(&user_id, current));
                *entry -= scheduled;
                segments.push(WorkSegment {
                    user: user_id,
                    date: current,
                    hours_worked: scheduled,
                });
                remaining -= scheduled;
            } else if strict && avail_full > EPSILON && scheduled < EPSILON {
                // A working day that cannot be fully claimed (another task is
                // using some of it).  In consecutive mode we must restart the
                // run: undo any segments accumulated so far and try again from
                // the next day.  Calendar gaps (avail_full == 0) do NOT break
                // the run because they are expected interruptions.
                for seg in segments.drain(..) {
                    let entry = state
                        .capacity
                        .entry((seg.user, seg.date))
                        .or_insert_with(|| self.hours_available(&seg.user, seg.date));
                    *entry += seg.hours_worked;
                }
                remaining = total_hours;
            }
            current += chrono::Duration::days(1);
        }

        segments
    }

    /// Allocate multiple workers on the same days in strict mode.
    /// Only schedules on days where every worker has enough remaining capacity
    /// for their respective daily cap.
    fn fill_slots_synchronized(
        &self,
        workers: &[(UserId, f32, Option<f32>, f32)],
        start_date: NaiveDate,
        state: &mut SchedulerState,
    ) -> Vec<WorkSegment> {
        let mut remaining: Vec<f32> = workers.iter().map(|&(_, _, _, total)| total).collect();
        let total_hours: Vec<f32> = workers.iter().map(|&(_, _, _, total)| total).collect();
        let mut segments: Vec<WorkSegment> = Vec::new();
        let mut current = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining.iter().any(|&r| r > EPSILON) && current <= limit {
            // Check that every worker with remaining hours can work on this day.
            // When no daily_cap is set (duration derived from workload), the
            // per-day expectation is the worker's full daily hours — not their
            // total remaining (which would be impossible to satisfy in one day).
            let all_can_work = workers
                .iter()
                .enumerate()
                .all(|(i, &(uid, _, daily_cap, _))| {
                    if remaining[i] <= EPSILON {
                        return true; // already done
                    }
                    let cap = daily_cap.unwrap_or_else(|| self.hours_available(&uid, current));
                    let avail = self.hours_remaining(state, uid, current);
                    avail >= cap - EPSILON
                });

            if all_can_work {
                for (i, &(uid, _, daily_cap, _)) in workers.iter().enumerate() {
                    if remaining[i] <= EPSILON {
                        continue;
                    }
                    let avail = self.hours_remaining(state, uid, current);
                    let cap = daily_cap.unwrap_or_else(|| self.hours_available(&uid, current));
                    let scheduled = cap.min(remaining[i]).min(avail);
                    if scheduled > EPSILON {
                        let entry = state
                            .capacity
                            .entry((uid, current))
                            .or_insert_with(|| self.hours_available(&uid, current));
                        *entry -= scheduled;
                        segments.push(WorkSegment {
                            user: uid,
                            date: current,
                            hours_worked: scheduled,
                        });
                        remaining[i] -= scheduled;
                    }
                }
            } else {
                // Check if any worker is blocked by another task (not a calendar gap).
                let any_blocked = workers
                    .iter()
                    .enumerate()
                    .any(|(i, &(uid, _, daily_cap, _))| {
                        if remaining[i] <= EPSILON {
                            return false;
                        }
                        let avail_full = self.hours_available(&uid, current);
                        if avail_full <= EPSILON {
                            return false; // calendar gap — not a block
                        }
                        let cap = daily_cap.unwrap_or(avail_full);
                        let avail = self.hours_remaining(state, uid, current);
                        avail < cap - EPSILON
                    });
                if any_blocked {
                    // Restart the run: undo accumulated segments and reset remaining.
                    for seg in segments.drain(..) {
                        let entry = state
                            .capacity
                            .entry((seg.user, seg.date))
                            .or_insert_with(|| self.hours_available(&seg.user, seg.date));
                        *entry += seg.hours_worked;
                    }
                    for (i, &tot) in total_hours.iter().enumerate() {
                        remaining[i] = tot;
                    }
                }
            }
            current += chrono::Duration::days(1);
        }

        segments
    }

    fn simulate_fill(
        &self,
        user_id: UserId,
        total_hours: f32,
        start_date: NaiveDate,
        max_per_day: Option<f32>,
        strict: bool,
        state: &SchedulerState,
    ) -> NaiveDate {
        let mut remaining = total_hours;
        let mut current = start_date;
        let mut last_date = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining > EPSILON && current <= limit {
            let avail_full = self.hours_available(&user_id, current);
            let scheduled = if strict {
                // Use daily hours as the cap when no explicit cap is set,
                // matching the fill_slot behaviour (avoids infinite restart loops
                // when total_hours spans multiple days).
                let cap = max_per_day.unwrap_or(avail_full);
                let avail = state
                    .capacity
                    .get(&(user_id, current))
                    .copied()
                    .unwrap_or(avail_full);
                if avail >= cap - EPSILON {
                    cap.min(remaining).min(avail)
                } else {
                    0.0
                }
            } else {
                let avail = state
                    .capacity
                    .get(&(user_id, current))
                    .copied()
                    .unwrap_or_else(|| self.hours_available(&user_id, current));
                if avail > EPSILON {
                    let cap = max_per_day.unwrap_or(f32::MAX);
                    avail.min(remaining).min(cap)
                } else {
                    0.0
                }
            };
            if scheduled > EPSILON {
                remaining -= scheduled;
                last_date = current;
            }
            current += chrono::Duration::days(1);
        }

        last_date
    }

    #[allow(clippy::too_many_arguments)]
    fn select_user_for_placeholder(
        &self,
        task_name: &str,
        required_tags: &HashSet<TagId>,
        workload_days: f32,
        earliest_start: NaiveDate,
        max_per_day: Option<f32>,
        strict: bool,
        state: &SchedulerState,
    ) -> Result<UserId, SchedulerError> {
        let total_hours = workload_days * self.default_schedule.hours_per_workload_day();
        let mut best_user: Option<UserId> = None;
        let mut best_end = NaiveDate::MAX;

        let mut eligible: Vec<UserId> = self
            .users_data
            .values()
            .map(|ud| &ud.user)
            .filter(|u| required_tags.is_subset(&u.tags))
            .map(|u| u.id)
            .collect();
        eligible.sort_by_key(|uid| uid.0);

        for uid in eligible {
            let end =
                self.simulate_fill(uid, total_hours, earliest_start, max_per_day, strict, state);
            if end < best_end {
                best_end = end;
                best_user = Some(uid);
            }
        }

        best_user.ok_or_else(|| SchedulerError::MissingTaskAffinity {
            task_name: task_name.to_string(),
            required_tags: required_tags.clone(),
        })
    }

    fn insert_task(
        &self,
        id: TaskId,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .get(&id)
            .expect("insert_task called with unknown TaskId");

        let earliest = self.earliest_start_from_dependencies(NodeId::Task(id), state);

        let start_date = match task.constraint {
            Some(c) if c.kind == constraint::ConstraintKind::Fixed => {
                if earliest > c.date {
                    state.allocations.constraint_violations.insert(
                        NodeId::Task(id),
                        ConstraintViolation {
                            node_name: task.name.clone(),
                            kind: ConstraintKind::Fixed,
                            required_date: c.date,
                            scheduled_date: earliest,
                        },
                    );
                    earliest
                } else {
                    c.date
                }
            }
            Some(c) if c.kind == constraint::ConstraintKind::Latest => {
                if earliest > c.date {
                    state.allocations.constraint_violations.insert(
                        NodeId::Task(id),
                        ConstraintViolation {
                            node_name: task.name.clone(),
                            kind: ConstraintKind::Latest,
                            required_date: c.date,
                            scheduled_date: earliest,
                        },
                    );
                }
                earliest
            }
            _ => earliest,
        };

        // Advance to the first working day on or after the computed start,
        // so the task never visually begins on a weekend or calendar holiday.
        let start_date = self.next_working_day_on_or_after(start_date);

        let mut time_allocation: Vec<WorkSegment> = Vec::new();
        let mut task_start: Option<NaiveDate> = None;
        let mut task_end: Option<NaiveDate> = None;

        let task_duration = task.duration_days_target;
        let strict = !task.relaxed_mode;
        let workers: Vec<WorkerSlot> = task.workers.clone();

        // Resolve all worker slots to (user_id, workload_days, daily_cap, total_hours).
        let mut resolved_workers: Vec<(UserId, f32, Option<f32>, f32)> = Vec::new();
        let hours_per_day = self.default_schedule.hours_per_workload_day();

        // The effective task duration is shared by all workers: it must be at
        // least as long as the most-loaded worker requires to stay within their
        // daily hours, and at least the requested duration_days_target.
        // For strict mode with multiple workers and no explicit duration, we still
        // derive a shared span from max(workload_days) so lighter workers are
        // spread evenly over the whole task rather than finishing early at full rate.
        let effective_duration: Option<f32> = if hours_per_day > EPSILON {
            let max_workload_days = workers
                .iter()
                .map(|slot| match slot {
                    WorkerSlot::Specific { workload_days, .. }
                    | WorkerSlot::Placeholder { workload_days, .. } => {
                        workload_days.ceil().max(1.0)
                    }
                })
                .fold(1.0f32, f32::max);

            if task_duration > 0.0 {
                // Explicit target: honour it but never exceed daily hours.
                Some(task_duration.ceil().max(max_workload_days))
            } else if strict && workers.len() > 1 {
                // No target but multi-worker strict: derive from heaviest worker.
                Some(max_workload_days)
            } else {
                // Single worker or relaxed with no target: fill at full daily rate.
                None
            }
        } else {
            None
        };

        for slot in &workers {
            let total_hours_for_slot = match slot {
                WorkerSlot::Specific { workload_days, .. }
                | WorkerSlot::Placeholder { workload_days, .. } => workload_days * hours_per_day,
            };
            // Spread each worker evenly over the shared effective duration so
            // all workers' workloads are distributed across the full task span.
            let daily_cap = effective_duration.map(|d| total_hours_for_slot / d);

            let (user_id, workload_days) = match slot {
                WorkerSlot::Specific {
                    user_id,
                    workload_days,
                } => (*user_id, *workload_days),
                WorkerSlot::Placeholder {
                    required_tags,
                    workload_days,
                } => {
                    let uid = self.select_user_for_placeholder(
                        &task.name,
                        required_tags,
                        *workload_days,
                        start_date,
                        daily_cap,
                        strict,
                        state,
                    )?;
                    (uid, *workload_days)
                }
            };

            let total_hours = workload_days * self.default_schedule.hours_per_workload_day();
            resolved_workers.push((user_id, workload_days, daily_cap, total_hours));
        }

        // In strict mode with multiple workers, allocate all workers on the same
        // days so their work stays synchronised.
        if strict && resolved_workers.len() > 1 {
            let segments = self.fill_slots_synchronized(&resolved_workers, start_date, state);
            for seg in &segments {
                task_start = Some(task_start.map_or(seg.date, |d: NaiveDate| d.min(seg.date)));
                task_end = Some(task_end.map_or(seg.date, |d: NaiveDate| d.max(seg.date)));
            }
            time_allocation.extend(segments);
        } else {
            for &(user_id, _workload_days, daily_cap, total_hours) in &resolved_workers {
                let segments =
                    self.fill_slot(user_id, total_hours, start_date, daily_cap, strict, state);

                if let Some(first) = segments.first() {
                    task_start =
                        Some(task_start.map_or(first.date, |d: NaiveDate| d.min(first.date)));
                }
                if let Some(last) = segments.last() {
                    task_end = Some(task_end.map_or(last.date, |d: NaiveDate| d.max(last.date)));
                }

                time_allocation.extend(segments);
            }
        }

        let min_end = if task_duration > 0.0 {
            self.advance_working_days(start_date, task_duration.ceil() as u32)
        } else {
            start_date
        };
        let task_start = task_start.unwrap_or(start_date);
        let mut task_end = task_end.map_or(min_end, |e| e.max(min_end));

        // Post-scheduling constraint check: worker unavailability can push task_start
        // past the constraint date even when the dependency-based earliest was within
        // the constraint. Overwrite any earlier (dependency-based) violation entry with
        // the accurate actual scheduled date.
        if let Some(c) = task.constraint {
            let violates = match c.kind {
                ConstraintKind::Fixed | ConstraintKind::Latest => task_start > c.date,
                ConstraintKind::Earliest => false,
            };
            if violates {
                state.allocations.constraint_violations.insert(
                    NodeId::Task(id),
                    ConstraintViolation {
                        node_name: task.name.clone(),
                        kind: c.kind,
                        required_date: c.date,
                        scheduled_date: task_start,
                    },
                );
            }
        }

        // For InProgress tasks, fill_slot already schedules from actual_start
        // (which may be in the past), so time_allocation contains all segments
        // including any past ones. Just preserve InProgress status.
        let (final_status, final_time_alloc) = if state.inprogress_ids.contains(&id) {
            // End date must be at least today so the Gantt bar runs to today and
            // dependent tasks are not scheduled from a past date.
            task_end = task_end.max(state.today);
            (Status::InProgress, time_allocation)
        } else {
            (Status::NotStarted, time_allocation)
        };

        state.allocations.tasks.insert(
            id,
            TaskState {
                status: final_status,
                allocation: TaskAllocation::Dynamic {
                    scheduled_start_date: task_start,
                    scheduled_end_date: task_end,
                    time_allocation: final_time_alloc,
                },
            },
        );
        state.inserted.insert(NodeId::Task(id));

        self.propagate_to_dependents(NodeId::Task(id), state, dependents_map, protect_node)?;

        Ok(())
    }

    /// Returns true if all of a milestone's predecessors are complete, meaning
    /// the milestone itself should be considered derived-complete and allowed to
    /// be scheduled at its dependency-computed date (even if that is in the past).
    ///
    /// - PlanStart is always considered complete.
    /// - A task predecessor is complete if its status is Complete or Dropped.
    ///
    /// Returns true if any direct dependency TASK has status InProgress.
    /// Milestone predecessors with InProgress derived status are intentionally ignored.
    fn milestone_derived_in_progress(&self, id: MilestoneId) -> bool {
        let Some(milestone) = self.milestones.get(&id) else {
            return false;
        };
        for dep in &milestone.dependencies {
            if let NodeId::Task(tid) = dep.id {
                let status = self
                    .node_allocations
                    .tasks
                    .get(&tid)
                    .map(|ts| ts.status)
                    .unwrap_or(Status::NotStarted);
                if status == Status::InProgress {
                    return true;
                }
            }
        }
        false
    }

    /// - A milestone predecessor is complete if its own derived_status is Complete
    ///   (set earlier in this scheduling pass, since we run in topological order).
    fn milestone_derived_complete(&self, id: MilestoneId, state: &SchedulerState) -> bool {
        let Some(milestone) = self.milestones.get(&id) else {
            return false;
        };
        for dep in &milestone.dependencies {
            match dep.id {
                NodeId::PlanStart => {} // always complete
                NodeId::Task(tid) => {
                    let status = self
                        .node_allocations
                        .tasks
                        .get(&tid)
                        .map(|ts| ts.status)
                        .unwrap_or(Status::NotStarted);
                    match status {
                        Status::Complete | Status::Dropped => {}
                        _ => return false,
                    }
                }
                NodeId::Milestone(mid) => {
                    let pred_complete = state
                        .allocations
                        .milestones
                        .get(&mid)
                        .map(|a| a.derived_status() == Status::Complete)
                        .unwrap_or(false);
                    if !pred_complete {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn insert_milestone(
        &self,
        id: MilestoneId,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        let milestone = self
            .milestones
            .get(&id)
            .expect("insert_milestone called with unknown MilestoneId");

        let earliest = self.earliest_start_from_dependencies(NodeId::Milestone(id), state);

        let date = match milestone.constraint {
            Some(c) if c.kind == constraint::ConstraintKind::Fixed => {
                if earliest > c.date {
                    state.allocations.constraint_violations.insert(
                        NodeId::Milestone(id),
                        ConstraintViolation {
                            node_name: milestone.name.clone(),
                            kind: ConstraintKind::Fixed,
                            required_date: c.date,
                            scheduled_date: earliest,
                        },
                    );
                    earliest
                } else {
                    c.date
                }
            }
            Some(c) if c.kind == constraint::ConstraintKind::Latest => {
                if earliest > c.date {
                    state.allocations.constraint_violations.insert(
                        NodeId::Milestone(id),
                        ConstraintViolation {
                            node_name: milestone.name.clone(),
                            kind: ConstraintKind::Latest,
                            required_date: c.date,
                            scheduled_date: earliest,
                        },
                    );
                }
                earliest
            }
            _ => earliest,
        };

        // Milestones must land on a day when the team is working.
        let date = self.next_working_day_on_or_after(date);

        // Post-scheduling constraint check: next_working_day_on_or_after may push
        // the milestone past the constraint date even when earliest was within it.
        if let Some(c) = milestone.constraint {
            let violates = match c.kind {
                ConstraintKind::Fixed | ConstraintKind::Latest => date > c.date,
                ConstraintKind::Earliest => false,
            };
            if violates {
                state.allocations.constraint_violations.insert(
                    NodeId::Milestone(id),
                    ConstraintViolation {
                        node_name: milestone.name.clone(),
                        kind: c.kind,
                        required_date: c.date,
                        scheduled_date: date,
                    },
                );
            }
        }

        state.allocations.milestones.insert(id, {
            let mut alloc = MilestoneAllocation::new(date);
            if self.milestone_derived_complete(id, state) {
                alloc.set_derived_status(Status::Complete);
            } else if self.milestone_derived_in_progress(id) {
                alloc.set_derived_status(Status::InProgress);
            }
            alloc
        });
        state.inserted.insert(NodeId::Milestone(id));

        self.propagate_to_dependents(NodeId::Milestone(id), state, dependents_map, protect_node)?;

        Ok(())
    }

    /// One compact pass: for every NotStarted task/milestone, try re-inserting it from its
    /// `earliest_start_from_dependencies`. If the result ends *earlier* than the current
    /// allocation (or, for tasks that already start at `earliest`, if gaps in the allocation
    /// can now be filled), keep the new allocation. Otherwise restore the old one.
    /// No forward propagation is triggered during compact — dependents that can also compact
    /// will be caught in subsequent passes.
    /// Returns true if any node improved.
    fn compact_pass(&self, state: &mut SchedulerState) -> Result<bool, SchedulerError> {
        let empty_dependents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut any_moved = false;

        // Collect compactable nodes sorted by start date (earliest first) so freed
        // capacity is immediately available to later tasks in the same pass.
        let mut node_info: Vec<(NodeId, NaiveDate, NaiveDate)> = Vec::new(); // (id, start, end)
        for (&tid, ts) in &state.allocations.tasks {
            if ts.status != Status::NotStarted {
                continue;
            }
            if let TaskAllocation::Dynamic {
                scheduled_start_date,
                scheduled_end_date,
                ..
            } = ts.allocation
            {
                node_info.push((NodeId::Task(tid), scheduled_start_date, scheduled_end_date));
            }
        }
        for (&mid, ma) in &state.allocations.milestones {
            node_info.push((NodeId::Milestone(mid), ma.date(), ma.date()));
        }
        node_info.sort_by_key(|&(_, start, _)| start);

        for (node_id, _old_start, old_end) in node_info {
            let earliest = self.earliest_start_from_dependencies(node_id, state);

            // Quick rejection: if the earliest possible start is at or after the
            // current end, there is no way to improve — skip the expensive reinsertion.
            if earliest >= old_end {
                continue;
            }

            match node_id {
                NodeId::Task(task_id) => {
                    // Save the current allocation and free its capacity.
                    let Some(old_ts) = state.allocations.tasks.remove(&task_id) else {
                        continue;
                    };
                    let old_segs = if let TaskAllocation::Dynamic {
                        ref time_allocation,
                        ..
                    } = old_ts.allocation
                    {
                        time_allocation.clone()
                    } else {
                        state.allocations.tasks.insert(task_id, old_ts);
                        continue; // Fixed allocation — skip
                    };
                    for seg in &old_segs {
                        *state.capacity.entry((seg.user, seg.date)).or_insert(0.0) +=
                            seg.hours_worked;
                    }
                    state.inserted.remove(&node_id);

                    // Try reinserting from the earliest possible date.
                    self.insert_task(task_id, state, &empty_dependents, None)?;

                    let new_end = state
                        .allocations
                        .tasks
                        .get(&task_id)
                        .map(|ts| ts.allocation.end_date())
                        .unwrap_or(old_end);

                    if new_end < old_end {
                        // Improvement — keep the new allocation.
                        any_moved = true;
                    } else {
                        // No improvement — restore the original allocation.
                        if let Some(new_ts) = state.allocations.tasks.remove(&task_id)
                            && let TaskAllocation::Dynamic {
                                ref time_allocation,
                                ..
                            } = new_ts.allocation
                        {
                            for seg in time_allocation {
                                *state.capacity.entry((seg.user, seg.date)).or_insert(0.0) +=
                                    seg.hours_worked;
                            }
                        }
                        state.inserted.remove(&node_id);
                        // Re-apply original segments to capacity.
                        for seg in &old_segs {
                            let entry = state
                                .capacity
                                .entry((seg.user, seg.date))
                                .or_insert_with(|| self.hours_available(&seg.user, seg.date));
                            *entry -= seg.hours_worked;
                        }
                        state.allocations.tasks.insert(task_id, old_ts);
                        state.inserted.insert(node_id);
                    }
                }
                NodeId::Milestone(mid) => {
                    // Don't compact milestones that have a Fixed constraint —
                    // they must stay on exactly the required date.
                    let has_fixed = self
                        .milestones
                        .get(&mid)
                        .and_then(|m| m.constraint)
                        .map(|c| c.kind == ConstraintKind::Fixed)
                        .unwrap_or(false);
                    if has_fixed {
                        continue;
                    }
                    let new_date = self.next_working_day_on_or_after(earliest);
                    if new_date < old_end {
                        state
                            .allocations
                            .milestones
                            .insert(mid, MilestoneAllocation::new(new_date));
                        any_moved = true;
                    }
                }
                NodeId::PlanStart => {}
            }
        }

        Ok(any_moved)
    }

    fn propagate_to_dependents(
        &self,
        node_id: NodeId,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        let Some(dependents) = dependents_map.get(&node_id) else {
            return Ok(());
        };

        let to_propagate: Vec<(NodeId, NaiveDate)> = dependents
            .iter()
            .filter_map(|&dep| {
                if Some(dep) == protect_node || !state.inserted.contains(&dep) {
                    return None;
                }
                let new_earliest = self.earliest_start_from_dependencies(dep, state);
                let current_start = self.node_start_date_in_state(dep, state)?;
                if new_earliest > current_start {
                    Some((dep, new_earliest))
                } else {
                    None
                }
            })
            .collect();

        for (dep, new_earliest) in to_propagate {
            self.propagate_forward(dep, new_earliest, state, dependents_map, protect_node)?;
        }

        Ok(())
    }

    fn node_start_date_in_state(
        &self,
        node_id: NodeId,
        state: &SchedulerState,
    ) -> Option<NaiveDate> {
        match node_id {
            NodeId::Task(tid) => state
                .allocations
                .tasks
                .get(&tid)
                .map(|ts| ts.allocation.start_date()),
            NodeId::Milestone(mid) => state.allocations.milestones.get(&mid).map(|a| a.date()),
            NodeId::PlanStart => Some(self.start_date),
        }
    }

    fn propagate_forward(
        &self,
        node_id: NodeId,
        _new_earliest: NaiveDate,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        if Some(node_id) == protect_node {
            return Ok(());
        }

        // Never move anchored (non-NotStarted) tasks
        if let NodeId::Task(tid) = node_id
            && self
                .node_allocations
                .tasks
                .get(&tid)
                .map(|ts| ts.status != Status::NotStarted)
                .unwrap_or(false)
        {
            return Ok(());
        }

        match node_id {
            NodeId::Task(tid) => {
                if let Some(ts) = state.allocations.tasks.remove(&tid)
                    && let TaskAllocation::Dynamic {
                        time_allocation, ..
                    } = &ts.allocation
                {
                    for seg in time_allocation {
                        let entry = state.capacity.entry((seg.user, seg.date)).or_insert(0.0);
                        *entry += seg.hours_worked;
                    }
                }
                state.inserted.remove(&node_id);
                self.insert_task(tid, state, dependents_map, protect_node)?;
            }
            NodeId::Milestone(mid) => {
                state.allocations.milestones.remove(&mid);
                state.inserted.remove(&node_id);
                self.insert_milestone(mid, state, dependents_map, protect_node)?;
            }
            NodeId::PlanStart => {}
        }

        Ok(())
    }
}
// }}}
// }}}

#[cfg(test)]
mod tests {
    use crate::data::{
        Milestone,
        constraint::{ConstraintKind, DateConstraint},
    };
    use crate::data::{
        NodeId, Plan, Task, User, WorkSchedule,
        allocation::{Status, TaskAllocation},
        dependency::Dependency,
    };
    use chrono::{Datelike, NaiveDate};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn segs_end(plan: &Plan, id: crate::data::TaskId) -> (Vec<NaiveDate>, NaiveDate) {
        if let TaskAllocation::Dynamic {
            time_allocation,
            scheduled_end_date,
            ..
        } = &plan.node_allocations.tasks[&id].allocation
        {
            let dates: Vec<NaiveDate> = time_allocation.iter().map(|s| s.date).collect();
            (dates, *scheduled_end_date)
        } else {
            (vec![], NaiveDate::MAX)
        }
    }

    /// Two independent strict tasks that need the same user at 4 h/day should share
    /// the available 8 h/day, not be serialised end-to-end.
    #[test]
    fn two_concurrent_tasks_share_days() {
        let plan_start = date(2026, 5, 4); // Monday
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays(); // 8h Mon-Fri

        let uid = plan.add_user(User::new("Alice"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        // Each task: 0.5 workload days, 1-day duration → cap = 0.5*8/1 = 4 h/day for 1 day.
        let mut a = Task::new("A", "");
        a.add_specific_worker(uid, 0.5);
        a.duration_days_target = 1.0;
        a.relaxed_mode = false;
        a.dependencies.push(dep_ps);
        let a_id = plan.add_task(a);

        let mut b = Task::new("B", "");
        b.add_specific_worker(uid, 0.5);
        b.duration_days_target = 1.0;
        b.relaxed_mode = false;
        b.dependencies.push(dep_ps);
        let b_id = plan.add_task(b);

        plan.compute_time_optimised_plan().unwrap();

        let (dates_a, end_a) = segs_end(&plan, a_id);
        let (dates_b, end_b) = segs_end(&plan, b_id);

        assert_eq!(dates_a.len(), 1, "Task A should have 1 segment");
        assert_eq!(dates_b.len(), 1, "Task B should have 1 segment");

        // Both tasks should land on the SAME day (each only needs 4h, user has 8h).
        assert_eq!(
            dates_a[0], dates_b[0],
            "Tasks A and B should share the same working day; A={}, B={}",
            dates_a[0], dates_b[0]
        );
        assert_eq!(end_a, end_b, "Both tasks should finish on the same day");
    }

    /// Strict task with two workers of different workloads (no explicit duration):
    /// the lighter worker must be spread over the full span driven by the heavier
    /// worker (4 days), not finish early at full rate.
    ///
    /// Setup: Alice 2 workload_days, Bob 4 workload_days, strict, duration=0.
    /// Expected: task spans 4 working days; Alice works 4h/day, Bob 8h/day.
    #[test]
    fn strict_multi_worker_lighter_worker_spreads_over_full_duration() {
        let plan_start = date(2026, 5, 4); // Monday
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays(); // 8h Mon-Fri

        let alice = plan.add_user(User::new("Alice"));
        let bob = plan.add_user(User::new("Bob"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 2.0); // 16h total
        task.add_specific_worker(bob, 4.0); // 32h total
        task.duration_days_target = 0.0; // derive from workload
        task.relaxed_mode = false; // strict: same days
        task.dependencies.push(dep_ps);
        let tid = plan.add_task(task);

        plan.compute_time_optimised_plan().unwrap();

        let alloc = &plan.node_allocations.tasks[&tid].allocation;
        let TaskAllocation::Dynamic {
            scheduled_start_date,
            scheduled_end_date,
            time_allocation,
        } = alloc
        else {
            panic!("expected Dynamic allocation");
        };

        // Task should span exactly 4 working days (Mon–Thu).
        let expected_start = date(2026, 5, 4);
        let expected_end = date(2026, 5, 7);
        assert_eq!(
            *scheduled_start_date, expected_start,
            "task should start Monday 4 May"
        );
        assert_eq!(
            *scheduled_end_date, expected_end,
            "task should end Thursday 7 May"
        );

        // Alice should have 4 segments (one per day, 4h each).
        let alice_segs: Vec<_> = time_allocation.iter().filter(|s| s.user == alice).collect();
        assert_eq!(
            alice_segs.len(),
            4,
            "Alice should work on 4 days, not finish early"
        );
        for seg in &alice_segs {
            assert!(
                (seg.hours_worked - 4.0).abs() < 0.01,
                "Alice should work 4h/day, got {}h on {}",
                seg.hours_worked,
                seg.date
            );
        }

        // Bob should have 4 segments at 8h each.
        let bob_segs: Vec<_> = time_allocation.iter().filter(|s| s.user == bob).collect();
        assert_eq!(bob_segs.len(), 4, "Bob should work on 4 days");
        for seg in &bob_segs {
            assert!(
                (seg.hours_worked - 8.0).abs() < 0.01,
                "Bob should work 8h/day, got {}h on {}",
                seg.hours_worked,
                seg.date
            );
        }

        // Both workers must share the same 4 days.
        let alice_dates: std::collections::HashSet<_> = alice_segs.iter().map(|s| s.date).collect();
        let bob_dates: std::collections::HashSet<_> = bob_segs.iter().map(|s| s.date).collect();
        assert_eq!(
            alice_dates, bob_dates,
            "Alice and Bob must work on the same days"
        );
    }

    /// Compact pass fills gaps caused by forward-propagation:
    ///
    /// - HIGH priority task chain (PREREQ → DEP → CHAIN_END) is inserted first; PREREQ
    ///   occupies the first available day consuming all capacity.
    /// - LOW priority task (GAPPED) is inserted next: it starts on the same day as PREREQ
    ///   but PREREQ uses all capacity, so GAPPED is pushed later.
    /// - A "blocker" task at medium priority briefly occupies the days GAPPED needs;
    ///   the compact pass should detect the freed capacity and pull GAPPED back in.
    ///
    /// In practice this verifies that GAPPED ends no later than PREREQ's dependents
    /// finish — i.e. it isn't left stranded well past its natural completion window.
    #[test]
    fn compact_pulls_gapped_task_forward() {
        let plan_start = date(2026, 5, 4); // Monday
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alice"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        // BLOCKER: consumes all of Alice's capacity for 2 days (8h/day, workload=2, dur=2)
        // This has a "TAIL" dependent so its chain path is long → inserted first.
        let mut blocker = Task::new("BLOCKER", "");
        blocker.add_specific_worker(uid, 2.0); // 2*8 = 16 h total
        blocker.duration_days_target = 2.0; // cap = 16/2 = 8 h/day → fills whole day
        blocker.relaxed_mode = false;
        blocker.dependencies.push(dep_ps);
        let blocker_id = plan.add_task(blocker);

        // TAIL: depends on BLOCKER; makes BLOCKER's chain longer → priority > GAPPED
        let mut tail = Task::new("TAIL", "");
        tail.add_specific_worker(uid, 1.0);
        tail.duration_days_target = 1.0;
        tail.relaxed_mode = false;
        tail.dependencies
            .push(Dependency::new(NodeId::Task(blocker_id)));
        plan.add_task(tail);

        // GAPPED: same capacity needs as BLOCKER (8h/day, 2 days) but lower priority
        // (no dependents of its own beyond itself).  Without compact, it would be pushed
        // AFTER BLOCKER.  With compact, it should end on the SAME day as BLOCKER.
        let mut gapped = Task::new("GAPPED", "");
        gapped.add_specific_worker(uid, 2.0);
        gapped.duration_days_target = 2.0;
        gapped.relaxed_mode = false;
        gapped.dependencies.push(dep_ps);
        let gapped_id = plan.add_task(gapped);

        plan.compute_time_optimised_plan().unwrap();

        let (_, end_blocker) = segs_end(&plan, blocker_id);
        let (_, end_gapped) = segs_end(&plan, gapped_id);

        // Both tasks need exactly 2 days at 8h/day with no other constraints.
        // They CANNOT share days (each needs full capacity), but the compact pass must
        // ensure GAPPED does not end up serialised well after BLOCKER.
        // The two tasks need 4 working days total — they should finish by the end of the
        // first working week (Tue 2026-05-05 start → Fri 2026-05-08 end at the latest,
        // allowing for tomorrow's floor).
        let window_end = date(2026, 5, 8); // Fri May 8
        assert!(
            end_blocker <= window_end,
            "BLOCKER should finish by Fri May 8; got {}",
            end_blocker
        );
        assert!(
            end_gapped <= window_end,
            "GAPPED should finish by Fri May 8 after compact; got {}",
            end_gapped
        );
    }

    /// When a task is set to InProgress with no future work segments (e.g. it was
    /// just started today on a task with no prior allocation), the scheduler must
    /// reschedule its remaining work starting from today — not leave it as a
    /// zero-duration task ending today.
    #[test]
    fn inprogress_task_gets_rescheduled_from_today() {
        let today = chrono::Local::now().date_naive();
        // Plan start well in the past so the task isn't blocked by it.
        let plan_start = today - chrono::Duration::days(10);
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alice"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        let mut task = Task::new("IP", "");
        task.add_specific_worker(uid, 4.0); // 4 workload days → 32h total
        task.duration_days_target = 4.0; // explicit duration → daily cap = 8h/day
        task.relaxed_mode = false;
        task.dependencies.push(dep_ps);
        let task_id = plan.add_task(task);

        // Simulate starting the task (sets Fixed{start:today, end:today, segs:[]}).
        plan.start_task(task_id);

        // Schedule.
        plan.compute_time_optimised_plan().unwrap();

        let ts = &plan.node_allocations.tasks[&task_id];
        assert_eq!(
            ts.status,
            crate::data::allocation::Status::InProgress,
            "task should remain InProgress after rescheduling"
        );

        // The task must have actual future work segments (not zero-duration).
        let future_segs: Vec<_> = match &ts.allocation {
            TaskAllocation::Dynamic {
                time_allocation, ..
            } => time_allocation.iter().filter(|s| s.date >= today).collect(),
            TaskAllocation::Fixed {
                time_allocation, ..
            } => time_allocation.iter().filter(|s| s.date >= today).collect(),
        };
        assert!(
            !future_segs.is_empty(),
            "InProgress task should have future work segments after rescheduling"
        );

        let end_date = ts.allocation.end_date();
        assert!(
            end_date > today,
            "InProgress task should end after today (got {}), not be zero-duration",
            end_date
        );
    }

    /// InProgress tasks must claim their future capacity BEFORE any NotStarted
    /// task is scheduled, even when the NotStarted task has a longer critical
    /// path and would otherwise be sorted first.
    ///
    /// Scenario: one user, 8h/day.
    /// - ONGOING: InProgress, 4 workload-days remaining.  Needs all of the
    ///   user's capacity for the next 4 days.
    /// - CHAIN_A → CHAIN_B → CHAIN_C: a three-task chain (NotStarted), total
    ///   6 workload-days.  Its critical path is longer, so without Stage 1.5 it
    ///   would be sorted first and steal the days ONGOING needs.
    ///
    /// After scheduling ONGOING must end no later than `today + 4 working days`,
    /// confirming that its capacity was reserved before CHAIN_* ran.
    #[test]
    fn inprogress_scheduled_before_notstarted() {
        let today = chrono::Local::now().date_naive();
        let plan_start = today - chrono::Duration::days(5);
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alex"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        // ONGOING: 4 workload-days, started today (InProgress)
        let mut ongoing = Task::new("ONGOING", "");
        ongoing.add_specific_worker(uid, 4.0);
        ongoing.duration_days_target = 4.0;
        ongoing.relaxed_mode = false;
        ongoing.dependencies.push(dep_ps.clone());
        let ongoing_id = plan.add_task(ongoing);
        plan.start_task(ongoing_id);

        // CHAIN_A, CHAIN_B, CHAIN_C: three chained tasks totalling 6 days
        let mut chain_a = Task::new("CHAIN_A", "");
        chain_a.add_specific_worker(uid, 2.0);
        chain_a.duration_days_target = 2.0;
        chain_a.relaxed_mode = false;
        chain_a.dependencies.push(dep_ps.clone());
        let chain_a_id = plan.add_task(chain_a);

        let mut chain_b = Task::new("CHAIN_B", "");
        chain_b.add_specific_worker(uid, 2.0);
        chain_b.duration_days_target = 2.0;
        chain_b.relaxed_mode = false;
        chain_b
            .dependencies
            .push(Dependency::new(NodeId::Task(chain_a_id)));
        let chain_b_id = plan.add_task(chain_b);

        let mut chain_c = Task::new("CHAIN_C", "");
        chain_c.add_specific_worker(uid, 2.0);
        chain_c.duration_days_target = 2.0;
        chain_c.relaxed_mode = false;
        chain_c
            .dependencies
            .push(Dependency::new(NodeId::Task(chain_b_id)));
        plan.add_task(chain_c);

        plan.compute_time_optimised_plan().unwrap();

        let ts_ongoing = &plan.node_allocations.tasks[&ongoing_id];
        assert_eq!(
            ts_ongoing.status,
            crate::data::allocation::Status::InProgress,
            "ONGOING should remain InProgress"
        );

        // ONGOING has 4 workload-days.  With 8h/day it needs 4 working days.
        // Advance 4 working days from today to find the latest acceptable end.
        let mut deadline = today;
        let mut wd = 0;
        while wd < 4 {
            deadline += chrono::Duration::days(1);
            if deadline.weekday().number_from_monday() <= 5 {
                wd += 1;
            }
        }

        let ongoing_end = ts_ongoing.allocation.end_date();
        assert!(
            ongoing_end <= deadline,
            "ONGOING should finish within 4 working days of today (deadline {}); got {}",
            deadline,
            ongoing_end
        );
    }

    /// A milestone that is the plan target and has multiple predecessor tasks
    /// with varying critical-path lengths must be scheduled AFTER ALL of its
    /// predecessors complete — not just after the longest one.
    ///
    /// Bug: the old topological-sort-then-re-sort-by-crit-path approach put the
    /// milestone before short-path predecessors (same crit_to value as the
    /// longest predecessor), so those were not yet in the scheduler state when
    /// the milestone was inserted, causing it to land too early.
    #[test]
    fn target_milestone_after_all_predecessors() {
        // Plan starts Mon 2026-05-04.
        // Two tasks depend on PlanStart:
        //   LONG: 10 workload-days (critical path ≈ 10 days)
        //   SHORT: 2 workload-days (critical path ≈ 2 days)
        // Target milestone depends on BOTH.  It must land on or after LONG's end.
        let plan_start = date(2026, 5, 4); // Monday
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alice"));
        let dep_ps = Dependency::new(NodeId::PlanStart);

        let mut long_task = Task::new("LONG", "");
        long_task.add_specific_worker(uid, 10.0);
        long_task.duration_days_target = 10.0;
        long_task.relaxed_mode = false;
        long_task.dependencies.push(dep_ps.clone());
        let long_id = plan.add_task(long_task);

        let mut short_task = Task::new("SHORT", "");
        short_task.add_specific_worker(uid, 2.0);
        short_task.duration_days_target = 2.0;
        short_task.relaxed_mode = false;
        short_task.dependencies.push(dep_ps);
        let short_id = plan.add_task(short_task);

        let mut ms = Milestone::new("TARGET", "");
        ms.dependencies.push(Dependency::new(NodeId::Task(long_id)));
        ms.dependencies
            .push(Dependency::new(NodeId::Task(short_id)));
        let ms_id = plan.add_milestone(ms);
        plan.scheduler_target = NodeId::Milestone(ms_id);

        plan.compute_time_optimised_plan().unwrap();

        let long_end = plan.node_allocations.tasks[&long_id].allocation.end_date();
        let short_end = plan.node_allocations.tasks[&short_id].allocation.end_date();
        let ms_date = plan.node_allocations.milestones[&ms_id].date();

        assert!(
            ms_date >= long_end,
            "Milestone ({ms_date}) must be on or after LONG end ({long_end})"
        );
        assert!(
            ms_date >= short_end,
            "Milestone ({ms_date}) must be on or after SHORT end ({short_end})"
        );
    }

    #[test]
    fn fixed_milestone_not_moved_by_compact() {
        // Plan starts Mon 2026-05-04. Milestone fixed to Wed 2026-05-13.
        // A predecessor task completes by Tue 2026-05-05 (1 day), so the
        // earliest dependency-derived date is Wed 2026-05-06 — earlier than
        // the constraint.  Without the fix, compact would move the milestone
        // to 2026-05-06; with the fix it must stay on 2026-05-13.
        let plan_start = date(2026, 5, 4); // Monday
        let fixed_date = date(2026, 5, 13); // Wednesday the following week
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alice"));

        // Short predecessor task: 1 workload-day, finishes Tue 2026-05-05.
        let mut pred = Task::new("PRED", "");
        pred.add_specific_worker(uid, 1.0);
        pred.duration_days_target = 1.0;
        pred.relaxed_mode = false;
        pred.dependencies.push(Dependency::new(NodeId::PlanStart));
        let pred_id = plan.add_task(pred);

        // Milestone fixed to 2026-05-13.
        let mut ms = Milestone::new("MS", "");
        ms.constraint = Some(DateConstraint {
            kind: ConstraintKind::Fixed,
            date: fixed_date,
        });
        ms.dependencies.push(Dependency::new(NodeId::Task(pred_id)));
        let ms_id = plan.add_milestone(ms);

        plan.compute_time_optimised_plan().unwrap();

        let scheduled = plan.node_allocations.milestones[&ms_id].date();
        assert_eq!(
            scheduled, fixed_date,
            "Fixed milestone should stay on {fixed_date}, but was moved to {scheduled}"
        );
    }

    /// A Dropped middle task in a chain A → B(dropped) → C should be transparent:
    /// C must start the day after A ends, as if B had zero duration.
    #[test]
    fn dropped_task_is_transparent_to_dependents() {
        // Plan starts Mon 2026-05-04.
        //   A: 1 workload-day → finishes Mon 2026-05-04
        //   B: 3 workload-days → would normally occupy Tue–Thu 2026-05-05–07
        //   C: depends on B → would normally start Fri 2026-05-08
        //
        // After B is Dropped, C should start Tue 2026-05-05 (day after A ends).
        let plan_start = date(2026, 5, 4); // Monday
        let mut plan = Plan::new("test");
        plan.start_date = plan_start;
        plan.default_schedule = WorkSchedule::weekdays();

        let uid = plan.add_user(User::new("Alice"));

        let mut a = Task::new("A", "");
        a.add_specific_worker(uid, 1.0);
        a.dependencies.push(Dependency::new(NodeId::PlanStart));
        let a_id = plan.add_task(a);

        let mut b = Task::new("B", "");
        b.add_specific_worker(uid, 3.0);
        b.duration_days_target = 3.0; // daily_cap = 24h/3 = 8h/day → 3 full days
        b.dependencies.push(Dependency::new(NodeId::Task(a_id)));
        let b_id = plan.add_task(b);

        let mut c = Task::new("C", "");
        c.add_specific_worker(uid, 1.0);
        c.duration_days_target = 1.0;
        c.dependencies.push(Dependency::new(NodeId::Task(b_id)));
        let c_id = plan.add_task(c);

        // First pass: schedule normally so B gets a Dynamic allocation with real dates.
        plan.compute_time_optimised_plan().unwrap();

        // B should span Tue–Thu (3 days); C should start Fri.
        let b_end_normal = plan.node_allocations.tasks[&b_id].allocation.end_date();
        assert_eq!(
            b_end_normal,
            date(2026, 5, 7),
            "B should end Thu before drop"
        );

        // Drop B.
        plan.set_task_status(b_id, Status::Dropped);

        // Re-schedule.
        plan.compute_time_optimised_plan().unwrap();

        // A still ends Mon 2026-05-04.
        let a_end = plan.node_allocations.tasks[&a_id].allocation.end_date();
        assert_eq!(a_end, date(2026, 5, 4), "A should end Mon");

        // C must start the day after A ends (Tue 2026-05-05), not after B's old end.
        let c_start = plan.node_allocations.tasks[&c_id].allocation.start_date();
        assert_eq!(
            c_start,
            date(2026, 5, 5),
            "C should start Tue (day after A) when B is dropped, got {c_start}"
        );
    }
}
