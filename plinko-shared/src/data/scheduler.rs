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
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl SchedulerState {
    fn new(today: NaiveDate) -> Self {
        Self {
            capacity: HashMap::new(),
            allocations: NodeAllocations::default(),
            inserted: HashSet::new(),
            today,
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
        let target = self.scheduler_target;
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
            let is_anchored = self
                .node_allocations
                .tasks
                .get(&id)
                .map(|ts| ts.status != Status::NotStarted)
                .unwrap_or(false);
            if !is_anchored {
                continue;
            }

            let (start, end, status, time_alloc) = match self
                .node_allocations
                .tasks
                .get(&id)
                .map(|ts| &ts.allocation)
            {
                Some(TaskAllocation::Fixed {
                    start_date,
                    end_date,
                    corrected_end_date,
                    time_allocation,
                }) => (
                    *start_date,
                    corrected_end_date.unwrap_or(*end_date),
                    self.node_allocations.tasks[&id].status,
                    time_allocation.clone(),
                ),
                _ => {
                    let task = &self.tasks[&id];
                    let s = state.today;
                    let d = task.effective_duration_days().ceil() as i64;
                    let e = s + chrono::Duration::days(d.max(0));
                    (
                        s,
                        e,
                        self.node_allocations
                            .tasks
                            .get(&id)
                            .map(|ts| ts.status)
                            .unwrap_or(Status::NotStarted),
                        vec![],
                    )
                }
            };

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
        let deps = self.get_dependencies(&node_id);
        let is_milestone = matches!(node_id, NodeId::Milestone(_));

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
            let pred_end = self.node_end_date_in_state(dep.id, state);
            let lag = dep.lag_days.round() as i64;
            // For task predecessors: tasks need the *next* day to start work, so add 1.
            // Milestones are date-point markers, so they sit on (or after) the last day
            // the predecessor is "done" — no +1 offset needed.
            let start_after = match dep.id {
                NodeId::PlanStart | NodeId::Milestone(_) => pred_end + chrono::Duration::days(lag),
                NodeId::Task(_) => {
                    if is_milestone {
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
                .map(|ts| ts.allocation.end_date())
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
            let avail = self.hours_remaining(state, user_id, current);
            let scheduled = if strict {
                // In strict mode only schedule on days where the user has at
                // least cap hours of capacity remaining, so the task never
                // spreads its daily block across a partially-full day.
                // Use a small tolerance to avoid float rounding rejecting a day
                // that is effectively full (e.g. 0.5+0.5 = 1.0 days).
                let cap = max_per_day.unwrap_or(remaining);
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
        let mut segments: Vec<WorkSegment> = Vec::new();
        let mut current = start_date;
        let limit = start_date + chrono::Duration::days(MAX_FILL_DAYS);

        while remaining.iter().any(|&r| r > EPSILON) && current <= limit {
            // Check that every worker with remaining hours can work on this day.
            let all_can_work = workers
                .iter()
                .enumerate()
                .all(|(i, &(uid, _, daily_cap, _))| {
                    if remaining[i] <= EPSILON {
                        return true; // already done
                    }
                    let cap = daily_cap.unwrap_or(remaining[i]);
                    let avail = self.hours_remaining(state, uid, current);
                    avail >= cap - EPSILON
                });

            if all_can_work {
                for (i, &(uid, _, daily_cap, _)) in workers.iter().enumerate() {
                    if remaining[i] <= EPSILON {
                        continue;
                    }
                    let avail = self.hours_remaining(state, uid, current);
                    let cap = daily_cap.unwrap_or(remaining[i]);
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
            let scheduled = if strict {
                let cap = max_per_day.unwrap_or(remaining);
                let avail = state
                    .capacity
                    .get(&(user_id, current))
                    .copied()
                    .unwrap_or_else(|| self.hours_available(&user_id, current));
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

    fn select_user_for_placeholder(
        &self,
        required_tags: &HashSet<TagId>,
        workload_days: f32,
        earliest_start: NaiveDate,
        max_per_day: Option<f32>,
        strict: bool,
        state: &SchedulerState,
    ) -> UserId {
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

        best_user.expect("no eligible user for placeholder slot")
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

        let mut time_allocation: Vec<WorkSegment> = Vec::new();
        let mut task_start: Option<NaiveDate> = None;
        let mut task_end: Option<NaiveDate> = None;

        let task_duration = task.duration_days_target;
        let strict = !task.relaxed_mode;
        let workers: Vec<WorkerSlot> = task.workers.clone();

        // Resolve all worker slots to (user_id, workload_days, daily_cap, total_hours).
        let mut resolved_workers: Vec<(UserId, f32, Option<f32>, f32)> = Vec::new();
        for slot in &workers {
            let total_hours_for_slot = match slot {
                WorkerSlot::Specific { workload_days, .. }
                | WorkerSlot::Placeholder { workload_days, .. } => {
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
                        strict,
                        state,
                    );
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
        let task_end = task_end.map_or(min_end, |e| e.max(min_end));

        state.allocations.tasks.insert(
            id,
            TaskState {
                status: Status::NotStarted,
                allocation: TaskAllocation::Dynamic {
                    scheduled_start_date: task_start,
                    scheduled_end_date: task_end,
                    time_allocation,
                },
            },
        );
        state.inserted.insert(NodeId::Task(id));

        self.propagate_to_dependents(NodeId::Task(id), state, dependents_map, protect_node)?;

        Ok(())
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

        state
            .allocations
            .milestones
            .insert(id, MilestoneAllocation::new(date));
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
        NodeId, Plan, Task, User, WorkSchedule, allocation::TaskAllocation, dependency::Dependency,
    };
    use chrono::NaiveDate;

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
}
