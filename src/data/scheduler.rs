use crate::data::allocation::{
    MilestoneAllocation, PlanAllocation, SlotAllocation, TaskAllocation, WorkSegment,
};
use crate::data::ids::TagId;
use crate::data::task::{TaskStatus, WorkerSlot};
use crate::data::{Dependency, MilestoneId, NodeId, Plan, TaskId, UserId, constraint};
use chrono::NaiveDate;
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

type NodeChain = Vec<NodeId>;

/// Amount of remaining work below which we consider a slot fully filled.
const EPSILON: f32 = 1e-6;

/// Maximum calendar days to search forward when filling a slot.
/// Guards against infinite loops when a user has no working days at all.
const MAX_FILL_DAYS: i64 = 3_650; // ~10 years

#[derive(Debug, Clone)]
pub enum SchedulerError {
    EmptyChain,
    MissingTaskAffinity {
        task_name: String,
        required_tags: HashSet<TagId>,
    },
    NoPathsToNode(NodeId),
    FixedConstraintViolated {
        task_name: String,
        required_date: NaiveDate,
        earliest_possible: NaiveDate,
    },
    LatestConstraintViolated {
        task_name: String,
        deadline: NaiveDate,
        computed_start: NaiveDate,
    },
    DisconnectedNode(NodeId),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SchedulerError::EmptyChain => write!(f, "expected content in the node chain"),
            SchedulerError::MissingTaskAffinity {
                task_name,
                required_tags,
            } => {
                let mut tags: Vec<String> =
                    required_tags.iter().map(|id| id.0.to_string()).collect();
                tags.sort_unstable();
                write!(
                    f,
                    "task \"{task_name}\" is not satisfied, needs the following tags: {}",
                    tags.join(", ")
                )
            }
            SchedulerError::NoPathsToNode(node_id) => {
                write!(f, "no path from plan start to node {node_id:?}")
            }
            SchedulerError::FixedConstraintViolated {
                task_name,
                required_date,
                earliest_possible,
            } => write!(
                f,
                "task \"{task_name}\" has a Fixed constraint on {required_date} but \
                 cannot start before {earliest_possible}",
            ),
            SchedulerError::LatestConstraintViolated {
                task_name,
                deadline,
                computed_start,
            } => write!(
                f,
                "task \"{task_name}\" must start by {deadline} but the earliest \
                 possible start is {computed_start}",
            ),
            SchedulerError::DisconnectedNode(node_id) => {
                write!(f, "node {node_id:?} has no path back to PlanStart")
            }
        }
    }
}

// ── Transient scheduler state (not serialised) ────────────────────────────────

struct SchedulerState {
    /// Remaining capacity per (user, date). Seeded lazily from `Plan::hours_available`.
    capacity: HashMap<(UserId, NaiveDate), f32>,
    /// Allocation being built up during the run.
    allocation: PlanAllocation,
    /// Set of nodes that have been fully inserted into `allocation`.
    inserted: HashSet<NodeId>,
    /// Today's date — used to floor freshly-scheduled tasks so they never start in the past.
    today: NaiveDate,
}

impl SchedulerState {
    fn new(today: NaiveDate) -> Self {
        Self {
            capacity: HashMap::new(),
            allocation: PlanAllocation::new(),
            inserted: HashSet::new(),
            today,
        }
    }
}

// ── Plan impl ─────────────────────────────────────────────────────────────────

impl Plan {
    /// Run the time-optimised scheduler, storing the result in `self.allocation`
    /// and updating `self.dates` for UI back-compat.
    pub fn compute_time_optimised_plan(&mut self) -> Result<(), SchedulerError> {
        let today = chrono::Local::now().date_naive();

        // Stretch any overrunning InProgress tasks so dependent NotStarted
        // tasks never schedule around a stale past end date. We do this
        // inline (without clearing the allocation) so that pre_insert can
        // still use the existing allocation's end dates for on-track tasks.
        let in_progress_ids: Vec<TaskId> = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::InProgress)
            .map(|t| t.id)
            .collect();
        for id in in_progress_ids {
            let scheduled_end = {
                let task = &self.tasks[&id];
                task.actual_end_date
                    .or_else(|| {
                        self.allocation
                            .as_ref()
                            .and_then(|a| a.tasks.get(&id))
                            .map(|a| a.end_date)
                    })
                    .unwrap_or_else(|| {
                        let start = self.dates.task(&id).unwrap_or(today);
                        let d = task.effective_duration_days().ceil() as i64;
                        start + chrono::Duration::days(d.max(0))
                    })
            };
            if scheduled_end < today {
                self.tasks.get_mut(&id).unwrap().actual_end_date = Some(today);
            }
        }

        // Stage 1 – Validate
        self.all_tasks_completable()?;
        self.check_all_nodes_connected()?;
        let dependents_map = self.build_dependents_map();
        let mut state = SchedulerState::new(today);
        self.pre_insert_anchored_tasks(&mut state);

        // Stage 2 – Time-constrained nodes (soonest Fixed/Latest first)
        let time_constrained = self.get_time_constrained_nodes();
        for node in time_constrained {
            let list = self.get_priority_sorted_task_list_to_node(node)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, None)?;
                }
            }
        }

        // Stage 3 – scheduler_target dependents (skip if target == PlanStart)
        let target = self.scheduler_target;
        if !matches!(target, NodeId::PlanStart) {
            let list = self.get_priority_sorted_task_list_to_node(target)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, Some(target))?;
                }
            }
        }

        // Stage 4 – Remaining end nodes (must not push scheduler_target)
        let protect = if matches!(target, NodeId::PlanStart) {
            None
        } else {
            Some(target)
        };
        let list = self.get_priority_sorted_task_list_to_ends()?;
        for id in list {
            if !state.inserted.contains(&id) {
                self.insert_node(id, &mut state, &dependents_map, protect)?;
            }
        }

        // Commit
        self.dates = crate::data::StartDates::new();
        for (&tid, alloc) in &state.allocation.tasks {
            self.dates.set_task(tid, alloc.start_date);
        }
        for (&mid, alloc) in &state.allocation.milestones {
            self.dates.set_milestone(mid, alloc.date);
        }
        self.allocation = Some(state.allocation);
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Dispatch insertion to the correct handler based on node type.
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
            NodeId::PlanStart => Ok(()), // PlanStart is never "inserted"
        }
    }

    /// Verify that every task/milestone has at least one path back to PlanStart.
    fn check_all_nodes_connected(&self) -> Result<(), SchedulerError> {
        for &id in self.tasks.keys() {
            self.get_all_paths_to_node(NodeId::Task(id))
                .map_err(|_| SchedulerError::DisconnectedNode(NodeId::Task(id)))?;
        }
        for &id in self.milestones.keys() {
            self.get_all_paths_to_node(NodeId::Milestone(id))
                .map_err(|_| SchedulerError::DisconnectedNode(NodeId::Milestone(id)))?;
        }
        Ok(())
    }

    /// Remaining available hours for `user_id` on `date`, lazily initialised
    /// from `Plan::hours_available`.
    fn hours_remaining(&self, state: &mut SchedulerState, user_id: UserId, date: NaiveDate) -> f32 {
        *state
            .capacity
            .entry((user_id, date))
            .or_insert_with(|| self.hours_available(&user_id, date))
    }

    /// Seed all non-NotStarted tasks into state as fixed allocations.
    /// They consume no future capacity (work already happened) but appear in
    /// `state.inserted` so the main loop skips them, and their `end_date` is
    /// visible to dependent tasks.
    fn pre_insert_anchored_tasks(&self, state: &mut SchedulerState) {
        for (&id, task) in &self.tasks {
            if task.status == TaskStatus::NotStarted {
                continue;
            }

            let start = task
                .actual_start_date
                .or_else(|| self.dates.task(&id))
                .unwrap_or(state.today);
            let end = task
                .actual_end_date
                .or_else(|| {
                    self.allocation
                        .as_ref()
                        .and_then(|a| a.tasks.get(&id))
                        .map(|a| a.end_date)
                })
                .unwrap_or_else(|| {
                    let d = task.effective_duration_days().ceil() as i64;
                    start + chrono::Duration::days(d.max(0))
                });

            state.allocation.tasks.insert(
                id,
                TaskAllocation {
                    task_id: id,
                    slot_allocations: vec![],
                    start_date: start,
                    end_date: end,
                },
            );
            state.inserted.insert(NodeId::Task(id));
        }
    }

    /// Compute the earliest calendar date on which `node_id` can start, given
    /// the current `state` (predecessor end dates + lags). Also applies the
    /// `Earliest` constraint if present.
    fn earliest_start_from_dependencies(
        &self,
        node_id: NodeId,
        state: &SchedulerState,
    ) -> NaiveDate {
        let deps = self.get_dependencies(&node_id);
        let mut earliest = state.today;

        for dep in deps {
            let pred_end = self.node_end_date_in_state(dep.id, state);
            let lag = dep.lag_days.round() as i64;
            // For PlanStart / milestones (point-in-time anchors) the successor
            // starts on the same day as the anchor + lag.  For tasks (which
            // consume real days) the successor starts the calendar day *after*
            // the task finishes + lag.
            let start_after = match dep.id {
                NodeId::PlanStart | NodeId::Milestone(_) => pred_end + chrono::Duration::days(lag),
                NodeId::Task(_) => pred_end + chrono::Duration::days(lag + 1),
            };
            earliest = earliest.max(start_after);
        }

        // Apply actual_start_date as a floor (authoritative for non-NotStarted;
        // acts as a pinned earliest start for NotStarted tasks that have one set).
        if let NodeId::Task(id) = node_id
            && let Some(asd) = self.tasks.get(&id).and_then(|t| t.actual_start_date)
        {
            earliest = earliest.max(asd);
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

        earliest
    }

    /// Retrieve the "end date" of a node from the current state.
    ///
    /// - PlanStart: the plan start date
    /// - Milestone: the allocation date
    /// - Task: the allocation end_date
    ///
    /// Falls back to `self.start_date` if not yet allocated.
    fn node_end_date_in_state(&self, node_id: NodeId, state: &SchedulerState) -> NaiveDate {
        match node_id {
            NodeId::PlanStart => state.today.max(self.start_date),
            NodeId::Milestone(mid) => state
                .allocation
                .milestones
                .get(&mid)
                .map(|a| a.date)
                .unwrap_or(self.start_date),
            NodeId::Task(tid) => state
                .allocation
                .tasks
                .get(&tid)
                .map(|a| a.end_date)
                .unwrap_or(self.start_date),
        }
    }

    /// Fill `total_hours` worth of work for `user_id` starting from
    /// `start_date`, deducting from `state.capacity`. Returns the resulting
    /// `WorkSegment` list.
    ///
    /// `max_per_day` caps how many hours may be consumed on a single calendar day;
    /// when `Some`, work is spread across more days so the task spans its full
    /// `duration_days_target` even when `workload_days < duration_days`.
    fn fill_slot(
        &self,
        user_id: UserId,
        total_hours: f32,
        start_date: NaiveDate,
        max_per_day: Option<f32>,
        state: &mut SchedulerState,
    ) -> Vec<WorkSegment> {
        let mut remaining = total_hours;
        let mut segments: Vec<WorkSegment> = Vec::new();
        let mut current = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining > EPSILON && current <= limit {
            let avail = self.hours_remaining(state, user_id, current);
            if avail > EPSILON {
                let cap = max_per_day.unwrap_or(f32::MAX);
                let take = avail.min(remaining).min(cap);
                *state
                    .capacity
                    .entry((user_id, current))
                    .or_insert_with(|| self.hours_available(&user_id, current)) -= take;
                segments.push(WorkSegment {
                    date: current,
                    hours_worked: take,
                });
                remaining -= take;
            }
            current += chrono::Duration::days(1);
        }

        segments
    }

    /// Simulate filling `total_hours` for `user_id` from `start_date` without
    /// mutating state. Returns the last date that would receive work (i.e. the
    /// simulated `end_date`).
    fn simulate_fill(
        &self,
        user_id: UserId,
        total_hours: f32,
        start_date: NaiveDate,
        max_per_day: Option<f32>,
        state: &SchedulerState,
    ) -> NaiveDate {
        let mut remaining = total_hours;
        let mut current = start_date;
        let mut last_date = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining > EPSILON && current <= limit {
            let avail = state
                .capacity
                .get(&(user_id, current))
                .copied()
                .unwrap_or_else(|| self.hours_available(&user_id, current));
            if avail > EPSILON {
                let cap = max_per_day.unwrap_or(f32::MAX);
                let take = avail.min(remaining).min(cap);
                remaining -= take;
                last_date = current;
            }
            current += chrono::Duration::days(1);
        }

        last_date
    }

    /// From among users satisfying `required_tags`, pick the one who can finish
    /// `workload_days` of work the earliest (starting from `earliest_start`).
    /// Ties broken by smallest `UserId` (lexicographic on the inner Uuid).
    fn select_user_for_placeholder(
        &self,
        required_tags: &HashSet<TagId>,
        workload_days: f32,
        earliest_start: NaiveDate,
        max_per_day: Option<f32>,
        state: &SchedulerState,
    ) -> UserId {
        let total_hours = workload_days * self.default_schedule.hours_per_workload_day();

        let mut best_user: Option<UserId> = None;
        let mut best_end = NaiveDate::MAX;

        // Collect and sort eligible users for deterministic tie-breaking
        let mut eligible: Vec<UserId> = self
            .users
            .values()
            .filter(|u| required_tags.is_subset(&u.tags))
            .map(|u| u.id)
            .collect();
        eligible.sort_by_key(|uid| uid.0);

        for uid in eligible {
            let end = self.simulate_fill(uid, total_hours, earliest_start, max_per_day, state);
            if end < best_end {
                best_end = end;
                best_user = Some(uid);
            }
        }

        // Guaranteed by `all_tasks_completable` — at least one eligible user exists
        best_user.expect("no eligible user for placeholder slot")
    }

    /// Insert a task into the allocation. Checks constraints, fills each worker
    /// slot, and propagates forward to already-inserted dependents that need to
    /// move.
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
                    return Err(SchedulerError::FixedConstraintViolated {
                        task_name: task.name.clone(),
                        required_date: c.date,
                        earliest_possible: earliest,
                    });
                }
                c.date
            }
            Some(c) if c.kind == constraint::ConstraintKind::Latest => {
                if earliest > c.date {
                    return Err(SchedulerError::LatestConstraintViolated {
                        task_name: task.name.clone(),
                        deadline: c.date,
                        computed_start: earliest,
                    });
                }
                earliest
            }
            _ => earliest,
        };

        let mut slot_allocations: Vec<SlotAllocation> = Vec::new();
        let mut task_start: Option<NaiveDate> = None;
        let mut task_end: Option<NaiveDate> = None;

        // When duration_days_target is set, spread each slot's hours evenly over
        // the calendar duration so the task isn't packed into fewer days than intended.
        let task_duration = task.duration_days_target;

        // We need an immutable copy of workers to iterate while we mutate state
        let workers: Vec<WorkerSlot> = task.workers.clone();
        for slot in &workers {
            let total_hours_for_slot = match slot {
                WorkerSlot::Specific { workload_days, .. } => {
                    workload_days * self.default_schedule.hours_per_workload_day()
                }
                WorkerSlot::Placeholder { workload_days, .. } => {
                    workload_days * self.default_schedule.hours_per_workload_day()
                }
            };
            let daily_cap = if task_duration > 0.0 {
                Some(total_hours_for_slot / task_duration.ceil())
            } else {
                None
            };

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
                        required_tags,
                        *workload_days,
                        start_date,
                        daily_cap,
                        state,
                    );
                    (uid, *workload_days)
                }
            };

            let total_hours = workload_days * self.default_schedule.hours_per_workload_day();
            let segments = self.fill_slot(user_id, total_hours, start_date, daily_cap, state);

            if let Some(first) = segments.first() {
                task_start = Some(task_start.map_or(first.date, |d: NaiveDate| d.min(first.date)));
            }
            if let Some(last) = segments.last() {
                task_end = Some(task_end.map_or(last.date, |d: NaiveDate| d.max(last.date)));
            }

            slot_allocations.push(SlotAllocation { user_id, segments });
        }

        // Enforce that the task's calendar span is at least duration_days_target.
        // This matters when:
        //  - a task has workers but workload < duration (partial allocation per day)
        //  - a task has no workers (pure calendar block)
        let min_end = if task_duration > 0.0 {
            start_date + chrono::Duration::days((task_duration.ceil() as i64 - 1).max(0))
        } else {
            start_date
        };
        let task_start = task_start.unwrap_or(start_date);
        let task_end = task_end.map_or(min_end, |e| e.max(min_end));

        let alloc = TaskAllocation {
            task_id: id,
            slot_allocations,
            start_date: task_start,
            end_date: task_end,
        };

        state.allocation.tasks.insert(id, alloc);
        state.inserted.insert(NodeId::Task(id));

        // Propagate forward to already-inserted dependents
        self.propagate_to_dependents(NodeId::Task(id), state, dependents_map, protect_node)?;

        Ok(())
    }

    /// Insert a milestone into the allocation. Checks constraints and propagates
    /// forward to already-inserted dependents.
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
                    return Err(SchedulerError::FixedConstraintViolated {
                        task_name: milestone.name.clone(),
                        required_date: c.date,
                        earliest_possible: earliest,
                    });
                }
                c.date
            }
            Some(c) if c.kind == constraint::ConstraintKind::Latest => {
                if earliest > c.date {
                    return Err(SchedulerError::LatestConstraintViolated {
                        task_name: milestone.name.clone(),
                        deadline: c.date,
                        computed_start: earliest,
                    });
                }
                earliest
            }
            _ => earliest,
        };

        state.allocation.milestones.insert(
            id,
            MilestoneAllocation {
                milestone_id: id,
                date,
            },
        );
        state.inserted.insert(NodeId::Milestone(id));

        self.propagate_to_dependents(NodeId::Milestone(id), state, dependents_map, protect_node)?;

        Ok(())
    }

    /// After inserting `node_id`, check whether any already-inserted dependents
    /// need to move forward and call `propagate_forward` for each that does.
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

        // Collect dependents that need moving (snapshot to avoid borrow issues)
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

    /// Get the current start date of a node from state, if it has been inserted.
    fn node_start_date_in_state(
        &self,
        node_id: NodeId,
        state: &SchedulerState,
    ) -> Option<NaiveDate> {
        match node_id {
            NodeId::Task(tid) => state.allocation.tasks.get(&tid).map(|a| a.start_date),
            NodeId::Milestone(mid) => state.allocation.milestones.get(&mid).map(|a| a.date),
            NodeId::PlanStart => Some(self.start_date),
        }
    }

    /// Undo the existing allocation for `node_id` (re-crediting capacity),
    /// then re-insert it starting from `new_earliest`. Recurse to dependents
    /// that are pushed forward as a result.
    fn propagate_forward(
        &self,
        node_id: NodeId,
        _new_earliest: NaiveDate,
        state: &mut SchedulerState,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
        protect_node: Option<NodeId>,
    ) -> Result<(), SchedulerError> {
        if Some(node_id) == protect_node {
            return Ok(()); // Never move the protected node
        }

        // Never move anchored (non-NotStarted) tasks
        if let NodeId::Task(tid) = node_id
            && self
                .tasks
                .get(&tid)
                .map(|t| t.status != TaskStatus::NotStarted)
                .unwrap_or(false)
        {
            return Ok(());
        }

        // Undo existing allocation and re-credit capacity
        match node_id {
            NodeId::Task(tid) => {
                if let Some(alloc) = state.allocation.tasks.remove(&tid) {
                    for slot in &alloc.slot_allocations {
                        for seg in &slot.segments {
                            let entry = state
                                .capacity
                                .entry((slot.user_id, seg.date))
                                .or_insert(0.0);
                            *entry += seg.hours_worked;
                        }
                    }
                }
                state.inserted.remove(&node_id);
                self.insert_task(tid, state, dependents_map, protect_node)?;
            }
            NodeId::Milestone(mid) => {
                state.allocation.milestones.remove(&mid);
                state.inserted.remove(&node_id);
                self.insert_milestone(mid, state, dependents_map, protect_node)?;
            }
            NodeId::PlanStart => {}
        }

        Ok(())
    }

    /// Returns all nodes with `Fixed` or `Latest` constraints, sorted soonest-first.
    fn get_time_constrained_nodes(&self) -> Vec<NodeId> {
        let mut v: Vec<(NodeId, constraint::DateConstraint)> = self
            .tasks
            .iter()
            .filter_map(|(&id, task)| {
                task.constraint
                    .filter(|c| {
                        matches!(
                            c.kind,
                            constraint::ConstraintKind::Fixed | constraint::ConstraintKind::Latest
                        )
                    })
                    .map(|c| (NodeId::Task(id), c))
            })
            .chain(self.milestones.iter().filter_map(|(&id, milestone)| {
                milestone
                    .constraint
                    .filter(|c| {
                        matches!(
                            c.kind,
                            constraint::ConstraintKind::Fixed | constraint::ConstraintKind::Latest
                        )
                    })
                    .map(|c| (NodeId::Milestone(id), c))
            }))
            .collect();
        v.sort_by_key(|(_, c)| c.date);
        v.into_iter().map(|(id, _)| id).collect()
    }

    fn build_dependents_map(&self) -> HashMap<NodeId, Vec<NodeId>> {
        let mut map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (&task_id, task) in &self.tasks {
            let node = NodeId::Task(task_id);
            for dep in &task.dependencies {
                map.entry(dep.id).or_default().push(node);
            }
        }
        for (&milestone_id, milestone) in &self.milestones {
            let node = NodeId::Milestone(milestone_id);
            for dep in &milestone.dependencies {
                map.entry(dep.id).or_default().push(node);
            }
        }
        map
    }

    fn get_priority_sorted_task_list_to_node(
        &self,
        node_id: NodeId,
    ) -> Result<Vec<NodeId>, SchedulerError> {
        let sorted_paths = self.get_paths_to_node_sorted(node_id)?;
        let mut seen = HashSet::new();
        let sorted_task_list = sorted_paths
            .into_iter()
            .flatten()
            .filter(|node_id| seen.insert(*node_id))
            .collect();

        Ok(sorted_task_list)
    }

    fn get_priority_sorted_task_list_to_ends(&self) -> Result<Vec<NodeId>, SchedulerError> {
        let end_nodes = self.get_end_nodes();
        let mut all_paths_with_dur: Vec<(f32, NodeChain)> = end_nodes
            .iter()
            .map(|&node| self.get_all_paths_to_node(node))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .map(|p| (self.calculate_path_duration(&p), p))
            .collect();

        all_paths_with_dur
            .sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        let mut seen = HashSet::new();
        let sorted_task_list = all_paths_with_dur
            .into_iter()
            .flat_map(|(_, p)| p)
            .filter(|node_id| seen.insert(*node_id))
            .collect();

        Ok(sorted_task_list)
    }

    fn get_dependencies(&self, node_id: &NodeId) -> &[Dependency] {
        match node_id {
            NodeId::Task(task_id) => {
                &self
                    .tasks
                    .get(task_id)
                    .unwrap_or_else(|| panic!("cannot find expected node {node_id:?}"))
                    .dependencies
            }
            NodeId::Milestone(milestone_id) => {
                &self
                    .milestones
                    .get(milestone_id)
                    .unwrap_or_else(|| panic!("cannot find expected node {node_id:?}"))
                    .dependencies
            }
            NodeId::PlanStart => &[],
        }
    }

    fn get_all_paths_to_root(
        &self,
        current_chain: NodeChain,
    ) -> Result<Vec<NodeChain>, SchedulerError> {
        let node_id = current_chain
            .iter()
            .last()
            .ok_or(SchedulerError::EmptyChain)?;

        if matches!(node_id, NodeId::PlanStart) {
            return Ok(vec![current_chain]);
        }

        self.get_dependencies(node_id)
            .iter()
            .try_fold(vec![], |mut acc, dependency| {
                let mut new_chain = current_chain.clone();
                new_chain.push(dependency.id);
                acc.extend(self.get_all_paths_to_root(new_chain)?);
                Ok(acc)
            })
    }

    /// Returns the critical path from an end node to the root (PlanStart).
    /// The critical path is the longest path in terms of total duration
    /// (task durations + dependency lags).
    fn get_critical_path_to_root(&self) -> NodeChain {
        let end_nodes = self.get_end_nodes();
        if end_nodes.is_empty() {
            return vec![NodeId::PlanStart];
        }

        let all_paths: Vec<NodeChain> = end_nodes
            .iter()
            .flat_map(|end_node| {
                self.get_all_paths_to_root(vec![*end_node])
                    .unwrap_or_default()
            })
            .collect();

        if all_paths.is_empty() {
            return vec![NodeId::PlanStart];
        }

        all_paths
            .into_iter()
            .max_by(|a, b| {
                let dur_a = self.calculate_path_duration(a);
                let dur_b = self.calculate_path_duration(b);
                dur_a
                    .partial_cmp(&dur_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap() // Safe because we checked all_paths is not empty
    }

    /// Calculates the total duration of a path in working days.
    /// Sums up task durations (milestones = 0) and dependency lags.
    fn calculate_path_duration(&self, path: &NodeChain) -> f32 {
        let mut total_days = 0.0;

        for i in 0..path.len() {
            let current_node = path[i];

            // Add the duration of the current node
            match current_node {
                NodeId::Task(id) => {
                    if let Some(task) = self.tasks.get(&id) {
                        total_days += task.effective_duration_days();
                    }
                }
                NodeId::Milestone(_) | NodeId::PlanStart => {
                    // Milestones and PlanStart have zero duration
                }
            }

            if i + 1 < path.len() {
                let next_node = path[i + 1];

                let deps = self.get_dependencies(&current_node);
                if let Some(dep) = deps.iter().find(|d| d.id == next_node) {
                    total_days += dep.lag_days;
                }
            }
        }

        total_days
    }

    /// Returns all paths from PlanStart to `target` in arbitrary order.
    /// If `target` is `PlanStart`, returns a single chain `[PlanStart]`.
    /// Returns `Err(NoPathsToNode)` if the target has no path to `PlanStart`.
    fn get_all_paths_to_node(&self, target: NodeId) -> Result<Vec<NodeChain>, SchedulerError> {
        if matches!(target, NodeId::PlanStart) {
            return Ok(vec![vec![NodeId::PlanStart]]);
        }

        let paths: Vec<NodeChain> = self
            .get_all_paths_to_root(vec![target])
            .unwrap_or_default()
            .into_iter()
            .map(|mut p| {
                p.reverse();
                p
            })
            .collect();

        if paths.is_empty() {
            return Err(SchedulerError::NoPathsToNode(target));
        }

        Ok(paths)
    }

    /// Returns all paths from PlanStart to `target`, sorted by total duration
    /// (longest first). If `target` is `PlanStart`, returns a single chain
    /// containing only `PlanStart`. Returns `Err(NoPathsToNode)` if the target
    /// has no path back to `PlanStart`.
    fn get_paths_to_node_sorted(&self, target: NodeId) -> Result<Vec<NodeChain>, SchedulerError> {
        let mut paths_with_dur: Vec<(f32, NodeChain)> = self
            .get_all_paths_to_node(target)?
            .into_iter()
            .map(|p| (self.calculate_path_duration(&p), p))
            .collect();

        paths_with_dur
            .sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        Ok(paths_with_dur.into_iter().map(|(_, p)| p).collect())
    }

    /// Returns all nodes (tasks and milestones) that have no successors —
    /// i.e., nothing depends on them. These are the "end" or "leaf" nodes
    /// of the dependency graph.
    fn get_end_nodes(&self) -> Vec<NodeId> {
        let all_nodes: HashSet<NodeId> = self
            .tasks
            .keys()
            .map(|&id| NodeId::Task(id))
            .chain(self.milestones.keys().map(|&id| NodeId::Milestone(id)))
            .collect();

        let depended_upon: HashSet<NodeId> = self
            .tasks
            .values()
            .flat_map(|task| &task.dependencies)
            .chain(self.milestones.values().flat_map(|m| &m.dependencies))
            .map(|dep| dep.id)
            .collect();

        all_nodes.difference(&depended_upon).copied().collect()
    }

    /// Returns `Ok(())` if every placeholder worker slot on every task can be
    /// satisfied by at least one user in the plan.
    /// Specific slots are skipped — the user is already named.
    /// Returns the first unsatisfied placeholder's error otherwise.
    fn all_tasks_completable(&self) -> Result<(), SchedulerError> {
        use crate::data::task::WorkerSlot;
        let users: Vec<_> = self.users.values().collect();
        for task in self.tasks.values() {
            for slot in &task.workers {
                if let WorkerSlot::Placeholder { required_tags, .. } = slot
                    && !users.iter().any(|u| slot.is_satisfied_by(u))
                {
                    return Err(SchedulerError::MissingTaskAffinity {
                        task_name: task.name.clone(),
                        required_tags: required_tags.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Dependency, Milestone, Task, User};
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_plan() -> Plan {
        let mut p = Plan::new("Test");
        p.start_date = date(2030, 1, 7); // Monday — far future so today-floor never affects tests
        p
    }

    // ── get_end_nodes tests ───────────────────────────────────────────────────

    #[test]
    fn get_end_nodes_empty_plan() {
        let p = Plan::new("Empty");
        let end_nodes = p.get_end_nodes();
        assert!(end_nodes.is_empty());
    }

    #[test]
    fn get_end_nodes_single_task_no_dependencies() {
        let mut p = Plan::new("Single");
        let t = p.add_task(Task::new("T", ""));
        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t)));
    }

    #[test]
    fn get_end_nodes_linear_chain() {
        let mut p = Plan::new("Chain");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));

        // T1 -> T2 -> T3 (T3 is the end node)
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t3)));
    }

    #[test]
    fn get_end_nodes_multiple_end_nodes() {
        let mut p = Plan::new("Multiple");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));

        // T1 -> T2, T1 -> T3 (both T2 and T3 are end nodes)
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Task(t2)));
        assert!(end_nodes.contains(&NodeId::Task(t3)));
    }

    #[test]
    fn get_end_nodes_with_milestone() {
        let mut p = Plan::new("WithMilestone");
        let t1 = p.add_task(Task::new("T1", ""));
        let m = p.add_milestone(Milestone::new("Launch", ""));

        // T1 -> Milestone (milestone is the end node)
        p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Milestone(m)));
    }

    #[test]
    fn get_end_nodes_mixed_tasks_and_milestones() {
        let mut p = Plan::new("Mixed");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let m1 = p.add_milestone(Milestone::new("M1", ""));
        let m2 = p.add_milestone(Milestone::new("M2", ""));

        // T1 -> M1, T2 -> M2 (M1 and M2 are end nodes)
        p.add_milestone_dependency(m1, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_milestone_dependency(m2, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Milestone(m1)));
        assert!(end_nodes.contains(&NodeId::Milestone(m2)));
    }

    #[test]
    fn get_end_nodes_plan_start_dependency_doesnt_affect_result() {
        let mut p = Plan::new("PlanStart");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));

        // Both depend on PlanStart, but both are still end nodes
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
        assert!(end_nodes.contains(&NodeId::Task(t1)));
        assert!(end_nodes.contains(&NodeId::Task(t2)));
    }

    #[test]
    fn get_end_nodes_diamond_dependency() {
        let mut p = Plan::new("Diamond");
        let t1 = p.add_task(Task::new("T1", ""));
        let t2 = p.add_task(Task::new("T2", ""));
        let t3 = p.add_task(Task::new("T3", ""));
        let t4 = p.add_task(Task::new("T4", ""));

        //     T1
        //    /  \
        //   T2  T3
        //    \  /
        //     T4
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t2)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t3)))
            .unwrap();

        let end_nodes = p.get_end_nodes();
        assert_eq!(end_nodes.len(), 1);
        assert!(end_nodes.contains(&NodeId::Task(t4)));
    }

    // ── Critical Path Tests ───────────────────────────────────────────────────

    #[test]
    fn critical_path_empty_plan() {
        let p = Plan::new("Empty");
        let path = p.get_critical_path_to_root();
        assert_eq!(path, vec![NodeId::PlanStart]);
    }

    #[test]
    fn critical_path_single_task() {
        let mut p = Plan::new("Single");
        let t = p.add_task(Task::new("T", "").with_duration(5.0));
        p.add_task_dependency(t, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(path, vec![NodeId::Task(t), NodeId::PlanStart]);
    }

    #[test]
    fn critical_path_linear_chain() {
        let mut p = Plan::new("Chain");
        let t1 = p.add_task(Task::new("T1", "").with_duration(3.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0));
        let t3 = p.add_task(Task::new("T3", "").with_duration(2.0));

        // PlanStart -> T1 -> T2 -> T3
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(
            path,
            vec![
                NodeId::Task(t3),
                NodeId::Task(t2),
                NodeId::Task(t1),
                NodeId::PlanStart
            ]
        );

        // Duration should be 3 + 5 + 2 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_lag() {
        let mut p = Plan::new("WithLag");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(3.0));

        // PlanStart -> T1 -> (2 day lag) -> T2
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::with_lag(NodeId::Task(t1), 2.0))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Duration should be 5 + 2 (lag) + 3 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_lead() {
        let mut p = Plan::new("WithLead");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(3.0));

        // PlanStart -> T1 -> (1 day lead/overlap) -> T2
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::with_lead(NodeId::Task(t1), 1.0))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Duration should be 5 + (-1) (lead) + 3 = 7 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 7.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_chooses_longest_path() {
        let mut p = Plan::new("MultiplePaths");
        let t1 = p.add_task(Task::new("T1", "").with_duration(10.0)); // Long path
        let t2 = p.add_task(Task::new("T2", "").with_duration(2.0)); // Short path
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0)); // Convergence point

        // PlanStart -> T1 (10d) -> T3 (1d) = 11 days total
        // PlanStart -> T2 (2d)  -> T3 (1d) = 3 days total
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Should choose the longer path through T1
        assert!(path.contains(&NodeId::Task(t1)));
        assert!(!path.contains(&NodeId::Task(t2)));

        let duration = p.calculate_path_duration(&path);
        assert!((duration - 11.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_path_with_milestone() {
        let mut p = Plan::new("WithMilestone");
        let t1 = p.add_task(Task::new("T1", "").with_duration(5.0));
        let m = p.add_milestone(Milestone::new("Launch", ""));

        // PlanStart -> T1 -> Milestone (0 duration)
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_milestone_dependency(m, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        assert_eq!(
            path,
            vec![NodeId::Milestone(m), NodeId::Task(t1), NodeId::PlanStart]
        );

        // Duration should be 5 (milestone has 0 duration)
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 5.0).abs() < f32::EPSILON);
    }

    // ── get_paths_to_node_sorted Tests ───────────────────────────────────────

    // ── all_tasks_completable Tests ───────────────────────────────────────────

    #[test]
    fn all_completable_empty_plan() {
        let p = Plan::new("Empty");
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_task_with_no_workers() {
        let mut p = Plan::new("NoWorkers");
        p.add_task(Task::new("T", ""));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_task_with_only_specific_workers() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Specific");
        let rust = p.add_tag("rust").unwrap();
        let uid = p.add_user(User::new("Alice").with_tag(rust));
        let mut task = Task::new("T", "");
        task.add_specific_worker(uid, 3.0);
        p.add_task(task);
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_placeholder_satisfied_by_one_user() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Satisfied");
        let rust = p.add_tag("rust").unwrap();
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([rust], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag(rust));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_fails_when_no_user_satisfies_placeholder() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Unsatisfied");
        let rust = p.add_tag("rust").unwrap();
        let python = p.add_tag("python").unwrap();
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([rust], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag(python));
        let err = p.all_tasks_completable().unwrap_err();
        assert!(matches!(err, SchedulerError::MissingTaskAffinity { .. }));
        assert!(err.to_string().contains("\"T\""));
    }

    #[test]
    fn all_completable_no_users_but_placeholder_required() {
        use crate::data::TagId;
        let mut p = Plan::new("NoUsers");
        let rust = p.add_tag("rust").unwrap();
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([rust], 3.0);
        p.add_task(task);
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn all_completable_partial_match_is_insufficient() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Partial");
        let rust = p.add_tag("rust").unwrap();
        let skia = p.add_tag("skia").unwrap();
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([rust, skia], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag(rust)); // missing skia
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn all_completable_second_user_satisfies_placeholder() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("SecondUser");
        let rust = p.add_tag("rust").unwrap();
        let python = p.add_tag("python").unwrap();
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([rust], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice").with_tag(python));
        p.add_user(User::new("Bob").with_tag(rust));
        assert!(p.all_tasks_completable().is_ok());
    }

    #[test]
    fn all_completable_display_lists_tags_sorted() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Display");
        let t1 = p.add_tag("typescript").unwrap();
        let t2 = p.add_tag("react").unwrap();
        let mut task = Task::new("Frontend", "");
        task.add_placeholder_worker([t1, t2], 3.0);
        p.add_task(task);
        p.add_user(User::new("Alice")); // no tags
        let msg = p.all_tasks_completable().unwrap_err().to_string();
        assert!(msg.contains("\"Frontend\""));
        // IDs are in the message (sorted UUIDs)
        assert!(msg.contains(t1.0.to_string().as_str()) || msg.contains(t2.0.to_string().as_str()));
    }

    #[test]
    fn all_completable_mixed_slots_both_must_pass() {
        use crate::data::{TagId, User};
        let mut p = Plan::new("Mixed");
        let design = p.add_tag("design").unwrap();
        let rust = p.add_tag("rust").unwrap();
        let uid = p.add_user(User::new("Alice").with_tag(design));
        let mut task = Task::new("T", "");
        task.add_specific_worker(uid, 2.0);
        task.add_placeholder_worker([rust], 3.0); // no rust user
        p.add_task(task);
        assert!(p.all_tasks_completable().is_err());
    }

    #[test]
    fn paths_to_node_plan_start_returns_single_root_chain() {
        let p = Plan::new("Empty");
        let paths = p.get_paths_to_node_sorted(NodeId::PlanStart).unwrap();
        assert_eq!(paths, vec![vec![NodeId::PlanStart]]);
    }

    #[test]
    fn paths_to_node_disconnected_returns_error() {
        let mut p = Plan::new("Disconnected");
        let t = p.add_task(Task::new("T", "").with_duration(3.0));
        // T has no dependency on PlanStart — no path to root
        let err = p.get_paths_to_node_sorted(NodeId::Task(t)).unwrap_err();
        assert!(matches!(err, SchedulerError::NoPathsToNode(_)));
    }

    #[test]
    fn paths_to_node_single_path() {
        let mut p = Plan::new("Linear");
        let t1 = p.add_task(Task::new("T1", "").with_duration(3.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0));
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();

        let paths = p.get_paths_to_node_sorted(NodeId::Task(t2)).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0],
            vec![NodeId::PlanStart, NodeId::Task(t1), NodeId::Task(t2)]
        );
    }

    #[test]
    fn paths_to_node_sorted_by_duration() {
        let mut p = Plan::new("MultiplePaths");
        let t1 = p.add_task(Task::new("T1", "").with_duration(10.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(2.0));
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0));

        // PlanStart -> T1 (10d) -> T3 = 11 days
        // PlanStart -> T2  (2d) -> T3 =  3 days
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t2)))
            .unwrap();

        let paths = p.get_paths_to_node_sorted(NodeId::Task(t3)).unwrap();
        assert_eq!(paths.len(), 2);

        // Longest first
        let dur0 = p.calculate_path_duration(&paths[0]);
        let dur1 = p.calculate_path_duration(&paths[1]);
        assert!((dur0 - 11.0).abs() < f32::EPSILON);
        assert!((dur1 - 3.0).abs() < f32::EPSILON);
        assert!(paths[0].contains(&NodeId::Task(t1)));
        assert!(paths[1].contains(&NodeId::Task(t2)));
    }

    #[test]
    fn critical_path_diamond_pattern() {
        let mut p = Plan::new("Diamond");
        let t1 = p.add_task(Task::new("T1", "").with_duration(2.0));
        let t2 = p.add_task(Task::new("T2", "").with_duration(5.0)); // Longer branch
        let t3 = p.add_task(Task::new("T3", "").with_duration(1.0)); // Shorter branch
        let t4 = p.add_task(Task::new("T4", "").with_duration(3.0));

        //       T1 (2d)
        //      /      \
        //   T2 (5d)  T3 (1d)
        //      \      /
        //       T4 (3d)
        p.add_task_dependency(t1, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t3, Dependency::new(NodeId::Task(t1)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t2)))
            .unwrap();
        p.add_task_dependency(t4, Dependency::new(NodeId::Task(t3)))
            .unwrap();

        let path = p.get_critical_path_to_root();
        // Should go through T2 (longer): T4 -> T2 -> T1 -> PlanStart
        assert!(path.contains(&NodeId::Task(t2)));
        assert!(!path.contains(&NodeId::Task(t3)));

        // Duration: 3 + 5 + 2 = 10 days
        let duration = p.calculate_path_duration(&path);
        assert!((duration - 10.0).abs() < f32::EPSILON);
    }

    // ── compute_time_optimised_plan tests ─────────────────────────────────────

    /// Plan start: 2030-01-07 (Monday, future). Standard 5-day week, 8 h/day.
    /// Single task T: 1 workload-day for Alice → fills Monday 8 h.
    #[test]
    fn scheduler_single_task_one_user() {
        let mut p = make_plan();
        p.start_date = date(2030, 1, 7); // future Monday
        let alice = p.add_user(User::new("Alice"));

        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 1.0);
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let ta = &alloc.tasks[&tid];

        assert_eq!(ta.start_date, date(2030, 1, 7));
        assert_eq!(ta.end_date, date(2030, 1, 7));
        assert_eq!(ta.slot_allocations.len(), 1);
        assert_eq!(ta.slot_allocations[0].segments.len(), 1);
        assert_eq!(ta.slot_allocations[0].segments[0].date, date(2030, 1, 7));
        assert!((ta.slot_allocations[0].segments[0].hours_worked - 8.0).abs() < EPSILON);
    }

    /// Two sequential tasks: T1 (1 day) then T2 (1 day). T2 should start the
    /// day after T1 ends (Tuesday).
    #[test]
    fn scheduler_linear_dependency_start_dates() {
        let mut p = make_plan();
        p.start_date = date(2030, 1, 7); // future Monday
        let alice = p.add_user(User::new("Alice"));

        let mut t1 = Task::new("T1", "");
        t1.add_specific_worker(alice, 1.0);
        let t1id = p.add_task(t1);

        let mut t2 = Task::new("T2", "");
        t2.add_specific_worker(alice, 1.0);
        let t2id = p.add_task(t2);

        p.add_task_dependency(t1id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.add_task_dependency(t2id, Dependency::new(NodeId::Task(t1id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert_eq!(alloc.tasks[&t1id].start_date, date(2030, 1, 7)); // Mon
        assert_eq!(alloc.tasks[&t2id].start_date, date(2030, 1, 8)); // Tue
    }

    /// Weekend gap: 5 workload-days starting on Thursday → fills Thu, Fri,
    /// then Mon, Tue, Wed (skipping Sat/Sun).
    #[test]
    fn scheduler_weekend_gap_in_work_segments() {
        let mut p = make_plan();
        p.start_date = date(2030, 1, 10); // Thursday — far future so tests remain stable
        let alice = p.add_user(User::new("Alice"));

        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 5.0);
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let segs = &alloc.tasks[&tid].slot_allocations[0].segments;

        // Must have exactly 5 work segments (no weekend)
        assert_eq!(segs.len(), 5);
        let seg_dates: Vec<NaiveDate> = segs.iter().map(|s| s.date).collect();
        assert_eq!(seg_dates[0], date(2030, 1, 10)); // Thu
        assert_eq!(seg_dates[1], date(2030, 1, 11)); // Fri
        assert_eq!(seg_dates[2], date(2030, 1, 14)); // Mon (skipped Sat 12, Sun 13)
        assert_eq!(seg_dates[3], date(2030, 1, 15)); // Tue
        assert_eq!(seg_dates[4], date(2030, 1, 16)); // Wed
    }

    /// Placeholder slot: two candidates — Alice works 4 h/day (half time),
    /// Bob works the standard 8 h/day. The slot is 1 workload-day = 8 hours.
    /// Alice needs 2 calendar days (Mon+Tue); Bob needs 1 (Mon).
    /// The scheduler must pick Bob (earliest finisher) regardless of UUID order.
    #[test]
    fn scheduler_placeholder_picks_earliest_finisher() {
        use crate::data::{Weekday, WorkSchedule};

        let mut p = make_plan();
        p.start_date = date(2030, 1, 7); // future Monday

        let dev = p.add_tag("dev").unwrap();

        // Alice works only 4 h/day on every weekday
        let alice = p.add_user(User::new("Alice").with_tag(dev));
        let half_time = WorkSchedule::weekdays()
            .with_day(Weekday::Monday, 4.0)
            .with_day(Weekday::Tuesday, 4.0)
            .with_day(Weekday::Wednesday, 4.0)
            .with_day(Weekday::Thursday, 4.0)
            .with_day(Weekday::Friday, 4.0);
        p.set_user_schedule(alice, half_time);

        // Bob works the default 8 h/day
        let bob = p.add_user(User::new("Bob").with_tag(dev));

        // Placeholder task: 1 workload-day = 8 hours.
        // Alice: 4 h Mon + 4 h Tue → finishes Tuesday.
        // Bob:   8 h Mon            → finishes Monday.
        // Expected: Bob is selected.
        let mut task = Task::new("T", "");
        task.add_placeholder_worker([dev], 1.0);
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let ta = &alloc.tasks[&tid];
        assert_eq!(
            ta.slot_allocations[0].user_id, bob,
            "Bob should be selected because he finishes 1 workload-day one calendar day earlier"
        );
        // Bob finishes on Monday; task end_date should be Monday
        assert_eq!(ta.end_date, date(2030, 1, 7));
    }

    /// Latest constraint: task must start no later than Wednesday.
    #[test]
    fn scheduler_latest_constraint_respected() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));

        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 1.0);
        task.constraint = Some(crate::data::DateConstraint::latest(date(2030, 1, 9))); // Wed
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();
        let alloc = p.allocation.as_ref().unwrap();
        assert!(alloc.tasks[&tid].start_date <= date(2030, 1, 9));
    }

    /// Latest constraint violated: predecessor forces start after deadline.
    #[test]
    fn scheduler_latest_constraint_violated_returns_error() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));

        // T1: 3 workload-days (Mon–Wed)
        let mut t1 = Task::new("T1", "");
        t1.add_specific_worker(alice, 3.0);
        let t1id = p.add_task(t1);
        p.add_task_dependency(t1id, Dependency::new(NodeId::PlanStart))
            .unwrap();

        // T2: must start no later than Monday, but T1 doesn't finish until Wednesday
        let mut t2 = Task::new("T2", "");
        t2.add_specific_worker(alice, 1.0);
        t2.constraint = Some(crate::data::DateConstraint::latest(date(2026, 3, 9)));
        let t2id = p.add_task(t2);
        p.add_task_dependency(t2id, Dependency::new(NodeId::Task(t1id)))
            .unwrap();

        let err = p.compute_time_optimised_plan().unwrap_err();
        assert!(matches!(
            err,
            SchedulerError::LatestConstraintViolated { .. }
        ));
    }

    /// Fixed constraint respected.
    #[test]
    fn scheduler_fixed_constraint_sets_exact_start() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));

        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 1.0);
        task.constraint = Some(crate::data::DateConstraint::fixed(date(2030, 1, 9))); // Wed
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();
        let alloc = p.allocation.as_ref().unwrap();
        assert_eq!(alloc.tasks[&tid].start_date, date(2030, 1, 9));
    }

    /// Fixed constraint violated when earliest_possible > required_date.
    #[test]
    fn scheduler_fixed_constraint_violated_returns_error() {
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));

        // T1 takes 3 days (Mon–Wed). T2 fixed to start Monday → impossible.
        let mut t1 = Task::new("T1", "");
        t1.add_specific_worker(alice, 3.0);
        let t1id = p.add_task(t1);
        p.add_task_dependency(t1id, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let mut t2 = Task::new("T2", "");
        t2.add_specific_worker(alice, 1.0);
        t2.constraint = Some(crate::data::DateConstraint::fixed(date(2026, 3, 9)));
        let t2id = p.add_task(t2);
        p.add_task_dependency(t2id, Dependency::new(NodeId::Task(t1id)))
            .unwrap();

        let err = p.compute_time_optimised_plan().unwrap_err();
        assert!(matches!(
            err,
            SchedulerError::FixedConstraintViolated { .. }
        ));
    }

    /// Disconnected node (no path to PlanStart) → DisconnectedNode error.
    #[test]
    fn scheduler_disconnected_node_returns_error() {
        let mut p = make_plan();
        p.add_task(Task::new("Floating", "")); // no dependency

        let err = p.compute_time_optimised_plan().unwrap_err();
        assert!(matches!(err, SchedulerError::DisconnectedNode(_)));
    }

    /// Forward propagation: inserting T1 pushes T2's start forward correctly.
    #[test]
    fn scheduler_propagation_pushes_dependent_forward() {
        // Plan:
        //   PlanStart → T1 (3 days) → T3
        //   PlanStart → T2 (1 day)  → T3
        //
        // In longest-path ordering, T1 appears before T2 (path to T3 via T1
        // is longer).  T3 is inserted after T1 ends (Thursday).  When T2 is
        // then processed (it was already inserted by the T1 path), verify T3
        // correctly sits after both.  The key is that T3's start date must be
        // after the end of both T1 and T2.
        let mut p = make_plan();
        let alice = p.add_user(User::new("Alice"));
        let bob = p.add_user(User::new("Bob"));

        let mut t1 = Task::new("T1", "");
        t1.add_specific_worker(alice, 3.0); // Mon–Wed
        let t1id = p.add_task(t1);
        p.add_task_dependency(t1id, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let mut t2 = Task::new("T2", "");
        t2.add_specific_worker(bob, 1.0); // Mon only
        let t2id = p.add_task(t2);
        p.add_task_dependency(t2id, Dependency::new(NodeId::PlanStart))
            .unwrap();

        let mut t3 = Task::new("T3", "");
        t3.add_specific_worker(alice, 1.0);
        let t3id = p.add_task(t3);
        p.add_task_dependency(t3id, Dependency::new(NodeId::Task(t1id)))
            .unwrap();
        p.add_task_dependency(t3id, Dependency::new(NodeId::Task(t2id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        // T1 ends Wed 2026-03-11; T3 must start Thu at earliest
        assert!(alloc.tasks[&t3id].start_date >= date(2026, 3, 12));
    }

    /// Empty plan schedules successfully with empty allocation.
    #[test]
    fn scheduler_empty_plan_succeeds() {
        let mut p = make_plan();
        p.compute_time_optimised_plan().unwrap();
        let alloc = p.allocation.as_ref().unwrap();
        assert!(alloc.tasks.is_empty());
        assert!(alloc.milestones.is_empty());
    }

    // ── Anchored-task tests ───────────────────────────────────────────────────

    /// A Complete task with an existing date in plan.dates must not be rescheduled.
    #[test]
    fn scheduler_complete_task_is_not_rescheduled() {
        let mut p = Plan::new("Test");
        p.start_date = date(2025, 1, 1);

        let original_start = date(2025, 2, 3);
        let mut task = Task::new("Done", "");
        task.status = TaskStatus::Complete;
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(tid, original_start);

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert_eq!(
            alloc.tasks[&tid].start_date, original_start,
            "Complete task must retain its original start date"
        );
    }

    /// A NotStarted task on an old plan must start no earlier than today.
    #[test]
    fn scheduler_not_started_task_starts_today_when_plan_is_old() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Old plan");
        // Set plan start far in the past
        p.start_date = today - chrono::Duration::days(365);

        let alice = p.add_user(User::new("Alice"));
        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 1.0);
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert!(
            alloc.tasks[&tid].start_date >= today,
            "NotStarted task must not start before today"
        );
    }

    /// InProgress task must not be rescheduled.
    #[test]
    fn scheduler_in_progress_task_is_not_rescheduled() {
        let today = chrono::Local::now().date_naive();
        let mut p = Plan::new("Test");
        p.start_date = today - chrono::Duration::days(30);

        let original_start = today - chrono::Duration::days(5);
        let mut task = Task::new("WIP", "");
        task.status = TaskStatus::InProgress;
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(tid, original_start);

        p.compute_time_optimised_plan().unwrap();

        assert_eq!(
            p.allocation.as_ref().unwrap().tasks[&tid].start_date,
            original_start,
            "InProgress task must retain its original start date"
        );
    }

    /// OnHold task must not be rescheduled.
    #[test]
    fn scheduler_on_hold_task_is_not_rescheduled() {
        let today = chrono::Local::now().date_naive();
        let mut p = Plan::new("Test");
        p.start_date = today - chrono::Duration::days(30);

        let original_start = today - chrono::Duration::days(5);
        let mut task = Task::new("Paused", "");
        task.status = TaskStatus::OnHold;
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(tid, original_start);

        p.compute_time_optimised_plan().unwrap();

        assert_eq!(
            p.allocation.as_ref().unwrap().tasks[&tid].start_date,
            original_start,
            "OnHold task must retain its original start date"
        );
    }

    /// Dropped task must not be rescheduled.
    #[test]
    fn scheduler_dropped_task_is_not_rescheduled() {
        let today = chrono::Local::now().date_naive();
        let mut p = Plan::new("Test");
        p.start_date = today - chrono::Duration::days(30);

        let original_start = today - chrono::Duration::days(5);
        let mut task = Task::new("Cancelled", "");
        task.status = TaskStatus::Dropped;
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(tid, original_start);

        p.compute_time_optimised_plan().unwrap();

        assert_eq!(
            p.allocation.as_ref().unwrap().tasks[&tid].start_date,
            original_start,
            "Dropped task must retain its original start date"
        );
    }

    /// A Complete task's worker days must not block capacity for NotStarted tasks.
    /// Even if the anchor nominally spans many days, Alice's time-slots remain
    /// available because pre_insert stores empty slot_allocations.
    #[test]
    fn scheduler_complete_task_does_not_block_capacity() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Capacity");
        p.start_date = today - chrono::Duration::days(30);

        let alice = p.add_user(User::new("Alice"));

        // Complete task: Alice, large workload — would block Mon–Fri+ if capacity were consumed.
        let mut anchor = Task::new("BigAnchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.add_specific_worker(alice, 20.0); // 20 workload-days for Alice
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates
            .set_task(anchor_id, today - chrono::Duration::days(30));

        // NotStarted task: Alice, depends only on PlanStart.
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        p.add_task_dependency(follower_id, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        // Follower must start today — Alice's capacity was never consumed by the anchor.
        assert!(
            alloc.tasks[&follower_id].start_date <= today + chrono::Duration::days(4),
            "Follower should start near today; capacity was not consumed by the Complete anchor"
        );
    }

    /// When an anchored task has no prior allocation, pre_insert falls back to
    /// start + ceil(effective_duration_days). A dependent NotStarted task must
    /// use this derived end_date, not the start_date alone.
    #[test]
    fn scheduler_not_started_dependent_uses_derived_end_date_of_anchor() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("DerivedEnd");
        p.start_date = today - chrono::Duration::days(30);

        // Complete anchor: explicit 3-day duration, starts yesterday.
        // No prior allocation → end_date derived as anchor_start + 3.
        let anchor_start = today - chrono::Duration::days(1);
        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.duration_days_target = 3.0;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(anchor_id, anchor_start);

        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        p.add_task_dependency(follower_id, Dependency::new(NodeId::Task(anchor_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let derived_end = anchor_start + chrono::Duration::days(3);
        assert_eq!(
            alloc.tasks[&anchor_id].end_date, derived_end,
            "Anchor end_date must equal start + explicit duration"
        );
        // Follower starts the calendar day after the anchor ends (lag+1 rule).
        assert!(
            alloc.tasks[&follower_id].start_date > derived_end,
            "Follower must start after the derived anchor end_date"
        );
    }

    /// Positive lag after a Complete anchor is respected: follower starts
    /// anchor_end + lag + 1 days (or today, whichever is later).
    #[test]
    fn scheduler_lag_after_complete_anchor_is_respected() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Lag");
        p.start_date = today - chrono::Duration::days(30);

        // Anchor ends today - 4 (start today - 5, explicit 1-day duration).
        let anchor_start = today - chrono::Duration::days(5);
        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.duration_days_target = 1.0;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(anchor_id, anchor_start);

        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        // Lag of 14 calendar days after anchor end — pushes follower well past today.
        p.add_task_dependency(
            follower_id,
            Dependency::with_lag(NodeId::Task(anchor_id), 14.0),
        )
        .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let anchor_end = alloc.tasks[&anchor_id].end_date; // today - 4
        // start_after = anchor_end + lag + 1 = (today - 4) + 14 + 1 = today + 11
        let expected_earliest = anchor_end + chrono::Duration::days(15);
        assert!(
            alloc.tasks[&follower_id].start_date >= expected_earliest,
            "Follower must respect the lag after the Complete anchor"
        );
    }

    /// Negative lag (lead) after a Complete anchor: even if the lead would push
    /// the follower before today, the today-floor must still apply.
    #[test]
    fn scheduler_lead_after_complete_anchor_floored_to_today() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Lead");
        p.start_date = today - chrono::Duration::days(30);

        // Anchor ends yesterday (start today - 10, explicit 1-day duration).
        let anchor_start = today - chrono::Duration::days(10);
        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.duration_days_target = 1.0;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(anchor_id, anchor_start);

        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        // Lead of 3 days would nominally start follower 3 days before anchor ends — in the past.
        p.add_task_dependency(
            follower_id,
            Dependency::with_lead(NodeId::Task(anchor_id), 3.0),
        )
        .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert!(
            alloc.tasks[&follower_id].start_date >= today,
            "Lead must not push a NotStarted task before today"
        );
    }

    /// When plan.start_date is in the future, tasks should be anchored to
    /// start_date rather than today.
    #[test]
    fn scheduler_future_plan_start_date_is_floor_not_today() {
        let today = chrono::Local::now().date_naive();
        let future_start = today + chrono::Duration::days(30);

        let mut p = Plan::new("Future");
        p.start_date = future_start;

        let alice = p.add_user(User::new("Alice"));
        let mut task = Task::new("T", "");
        task.add_specific_worker(alice, 1.0);
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert!(
            alloc.tasks[&tid].start_date >= future_start,
            "Task must not start before the future plan start_date"
        );
        assert!(
            alloc.tasks[&tid].start_date >= today,
            "Task must also not start before today"
        );
    }

    /// An anchored task with no entry in plan.dates falls back to today as its
    /// start_date (and today + duration as its end_date).
    #[test]
    fn scheduler_anchored_task_with_no_date_falls_back_to_today() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("NoDate");
        p.start_date = today - chrono::Duration::days(30);

        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.duration_days_target = 2.0;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        // Deliberately do NOT call p.dates.set_task(anchor_id, ...)

        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        p.add_task_dependency(follower_id, Dependency::new(NodeId::Task(anchor_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        assert_eq!(
            alloc.tasks[&anchor_id].start_date, today,
            "Anchor with no plan.dates entry must fall back to today"
        );
        assert!(
            alloc.tasks[&follower_id].start_date > alloc.tasks[&anchor_id].end_date,
            "Follower must start after the anchor's derived end_date"
        );
    }

    /// A chain of Complete tasks followed by a NotStarted task: the NotStarted
    /// task must schedule after the last anchor's derived end_date.
    #[test]
    fn scheduler_chain_of_complete_tasks_followed_by_not_started() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Chain");
        p.start_date = today - chrono::Duration::days(30);

        // A: Complete, ends well in the past.
        let anchor_a_start = today - chrono::Duration::days(20);
        let mut task_a = Task::new("A", "");
        task_a.status = TaskStatus::Complete;
        task_a.duration_days_target = 1.0;
        let a_id = p.add_task(task_a);
        p.add_task_dependency(a_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(a_id, anchor_a_start);

        // B: Complete, ends in the future relative to today (start yesterday, 10-day span).
        let anchor_b_start = today - chrono::Duration::days(1);
        let mut task_b = Task::new("B", "");
        task_b.status = TaskStatus::Complete;
        task_b.duration_days_target = 10.0; // derived end = today + 9
        let b_id = p.add_task(task_b);
        p.add_task_dependency(b_id, Dependency::new(NodeId::Task(a_id)))
            .unwrap();
        p.dates.set_task(b_id, anchor_b_start);

        // C: NotStarted, depends on B.
        let alice = p.add_user(User::new("Alice"));
        let mut task_c = Task::new("C", "");
        task_c.add_specific_worker(alice, 1.0);
        let c_id = p.add_task(task_c);
        p.add_task_dependency(c_id, Dependency::new(NodeId::Task(b_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let b_end = alloc.tasks[&b_id].end_date; // today + 9
        let c_start = alloc.tasks[&c_id].start_date;

        assert!(c_start > b_end, "C must start after B's derived end_date");
        assert!(c_start >= today, "C must not start before today");
    }

    /// A milestone that depends on a Complete task must be placed after the
    /// anchor's end_date and no earlier than today.
    #[test]
    fn scheduler_milestone_after_complete_anchor() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Milestone");
        p.start_date = today - chrono::Duration::days(30);

        // Complete anchor: starts yesterday, 3-day explicit duration → ends today + 2.
        let anchor_start = today - chrono::Duration::days(1);
        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        anchor.duration_days_target = 3.0;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(anchor_id, anchor_start);

        let m_id = p.add_milestone(Milestone::new("Launch", ""));
        p.add_milestone_dependency(m_id, Dependency::new(NodeId::Task(anchor_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let anchor_end = alloc.tasks[&anchor_id].end_date; // today + 2
        let milestone_date = alloc.milestones[&m_id].date;

        assert!(
            milestone_date > anchor_end,
            "Milestone must be placed after the Complete anchor's end_date"
        );
        assert!(
            milestone_date >= today,
            "Milestone must not be placed before today"
        );
    }

    /// A NotStarted task that depends on a Complete anchor must start after the
    /// anchor's end date, but never before today.
    #[test]
    fn scheduler_not_started_dependent_starts_after_complete_anchor() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("Anchored");
        p.start_date = today - chrono::Duration::days(30);

        let anchor_start = today - chrono::Duration::days(10);

        // Complete task anchored to the past
        let mut anchor = Task::new("Anchor", "");
        anchor.status = TaskStatus::Complete;
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(anchor_id, anchor_start);

        // NotStarted task that depends on the anchor
        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        p.add_task_dependency(follower_id, Dependency::new(NodeId::Task(anchor_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let anchor_end = alloc.tasks[&anchor_id].end_date;
        let follower_start = alloc.tasks[&follower_id].start_date;

        assert!(
            follower_start >= today,
            "Follower must not start before today"
        );
        assert!(
            follower_start > anchor_end,
            "Follower must start after anchor's end date"
        );
    }

    /// compute_time_optimised_plan automatically stretches overrunning InProgress
    /// tasks without requiring a separate stretch_overrunning_tasks call.
    #[test]
    fn scheduler_auto_stretches_overrunning_in_progress_task() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("AutoStretch");
        p.start_date = today - chrono::Duration::days(30);

        // InProgress task that was supposed to end 5 days ago.
        let mut anchor = Task::new("Running", "");
        anchor.status = TaskStatus::InProgress;
        anchor.duration_days_target = 1.0; // derived end = start + 1
        let anchor_id = p.add_task(anchor);
        p.add_task_dependency(anchor_id, Dependency::new(NodeId::PlanStart))
            .unwrap();
        // Started 10 days ago → derived end = 9 days ago, clearly overdue.
        p.dates
            .set_task(anchor_id, today - chrono::Duration::days(10));

        let alice = p.add_user(User::new("Alice"));
        let mut follower = Task::new("Follower", "");
        follower.add_specific_worker(alice, 1.0);
        let follower_id = p.add_task(follower);
        p.add_task_dependency(follower_id, Dependency::new(NodeId::Task(anchor_id)))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        // The overrunning anchor must have been stretched to today.
        assert_eq!(
            alloc.tasks[&anchor_id].end_date, today,
            "Overrunning InProgress task must be stretched to today by the scheduler"
        );
        // The follower must start after today (i.e. tomorrow or later).
        assert!(
            alloc.tasks[&follower_id].start_date > today,
            "Follower must start after the stretched anchor end date"
        );
    }

    /// An on-track InProgress task (end date >= today) must not be stretched.
    #[test]
    fn scheduler_does_not_stretch_on_track_in_progress_task() {
        let today = chrono::Local::now().date_naive();

        let mut p = Plan::new("OnTrack");
        p.start_date = today - chrono::Duration::days(5);

        let mut task = Task::new("Running", "");
        task.status = TaskStatus::InProgress;
        task.duration_days_target = 20.0; // ends today + 15, well in the future
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();
        p.dates.set_task(tid, today - chrono::Duration::days(5));

        p.compute_time_optimised_plan().unwrap();

        assert_eq!(
            p.tasks[&tid].actual_end_date, None,
            "On-track InProgress task must not have actual_end_date set"
        );
    }

    // ── duration_days_target scheduling tests ─────────────────────────────────

    /// Task with explicit duration=2 and 1 workload-day for Alice (standard 8h/day).
    /// Expected: task spans 2 calendar days, 4 h/day (0.5 workload-days/day).
    #[test]
    fn scheduler_duration_target_spreads_workload_over_calendar_days() {
        let mut p = make_plan();
        p.start_date = date(2030, 1, 7); // Monday
        let alice = p.add_user(User::new("Alice"));

        let mut task = Task::new("T", "");
        task.duration_days_target = 2.0;
        task.add_specific_worker(alice, 1.0); // 1 workload-day = 8 hours total
        let tid = p.add_task(task);
        p.add_task_dependency(tid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();
        let ta = &alloc.tasks[&tid];

        // Calendar span must be 2 days (Mon -> Tue)
        assert_eq!(ta.start_date, date(2030, 1, 7), "task must start on Monday");
        assert_eq!(
            ta.end_date,
            date(2030, 1, 8),
            "task must end on Tuesday (2-day span)"
        );

        // Alice's workload must be spread: 2 segments of 4 h each (0.5 workload-day/day)
        let segs = &ta.slot_allocations[0].segments;
        assert_eq!(
            segs.len(),
            2,
            "Alice must have 2 work segments (one per calendar day)"
        );
        assert_eq!(segs[0].date, date(2030, 1, 7));
        assert!(
            (segs[0].hours_worked - 4.0).abs() < EPSILON,
            "4 h on day 1 (half of 8 h)"
        );
        assert_eq!(segs[1].date, date(2030, 1, 8));
        assert!(
            (segs[1].hours_worked - 4.0).abs() < EPSILON,
            "4 h on day 2 (half of 8 h)"
        );
    }

    /// A task with no workers and duration=2 must span 2 calendar days and must
    /// not consume any user's capacity on the days it covers.
    #[test]
    fn scheduler_no_worker_task_spans_duration_days_without_consuming_capacity() {
        let mut p = make_plan();
        p.start_date = date(2030, 1, 7); // Monday
        let alice = p.add_user(User::new("Alice"));

        // Calendar-only task: 2-day block, no workers
        let mut blocker = Task::new("Blocker", "");
        blocker.duration_days_target = 2.0;
        let bid = p.add_task(blocker);
        p.add_task_dependency(bid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        // Alice's task: 1 workload-day, depends only on PlanStart (runs in parallel)
        let mut worker_task = Task::new("Worker", "");
        worker_task.add_specific_worker(alice, 1.0);
        let wid = p.add_task(worker_task);
        p.add_task_dependency(wid, Dependency::new(NodeId::PlanStart))
            .unwrap();

        p.compute_time_optimised_plan().unwrap();

        let alloc = p.allocation.as_ref().unwrap();

        // Blocker must span Mon-Tue (2 calendar days)
        assert_eq!(alloc.tasks[&bid].start_date, date(2030, 1, 7));
        assert_eq!(
            alloc.tasks[&bid].end_date,
            date(2030, 1, 8),
            "no-worker task with duration=2 must have end_date = start + 1"
        );
        // Blocker must have no slot allocations (no workers)
        assert!(
            alloc.tasks[&bid].slot_allocations.is_empty(),
            "pure calendar task must have no slot allocations"
        );

        // Alice's task: starts on Monday - Blocker must not consume her capacity
        let worker_segs = &alloc.tasks[&wid].slot_allocations[0].segments;
        assert_eq!(
            worker_segs[0].date,
            date(2030, 1, 7),
            "Alice must be able to start on Monday - Blocker must not consume capacity"
        );
        assert!(
            (worker_segs[0].hours_worked - 8.0).abs() < EPSILON,
            "Alice must have her full 8 h on Monday"
        );
    }
}
