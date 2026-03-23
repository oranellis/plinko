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
        self.check_all_nodes_connected()?;
        let dependents_map = self.build_dependents_map();
        let mut state = SchedulerState::new(today);
        self.pre_insert_anchored_tasks(&mut state);

        // Stage 2 – Time-constrained nodes
        let time_constrained = self.get_time_constrained_nodes();
        for node in time_constrained {
            let list = self.get_priority_sorted_task_list_to_node(node)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, None)?;
                }
            }
        }

        // Stage 3 – scheduler_target dependents
        let target = self.scheduler_target;
        if !matches!(target, NodeId::PlanStart) {
            let list = self.get_priority_sorted_task_list_to_node(target)?;
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
        let list = self.get_priority_sorted_task_list_to_ends()?;
        for id in list {
            if !state.inserted.contains(&id) {
                self.insert_node(id, &mut state, &dependents_map, protect)?;
            }
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

        // If there are no dependencies, fall back to today (or plan start).
        let mut earliest = if deps.is_empty() {
            state.today.max(self.start_date)
        } else {
            self.start_date
        };

        for dep in deps {
            let pred_end = self.node_end_date_in_state(dep.id, state);
            let lag = dep.lag_days.round() as i64;
            let start_after = match dep.id {
                NodeId::PlanStart | NodeId::Milestone(_) => pred_end + chrono::Duration::days(lag),
                NodeId::Task(_) => pred_end + chrono::Duration::days(lag + 1),
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

        // Never schedule in the past — all unscheduled work starts no sooner than today.
        earliest = earliest.max(state.today);

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
                let cap = max_per_day.unwrap_or(remaining);
                if avail >= cap - EPSILON {
                    cap.min(remaining)
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
                    cap.min(remaining)
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
            let segments =
                self.fill_slot(user_id, total_hours, start_date, daily_cap, strict, state);

            if let Some(first) = segments.first() {
                task_start = Some(task_start.map_or(first.date, |d: NaiveDate| d.min(first.date)));
            }
            if let Some(last) = segments.last() {
                task_end = Some(task_end.map_or(last.date, |d: NaiveDate| d.max(last.date)));
            }

            time_allocation.extend(segments);
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

        state
            .allocations
            .milestones
            .insert(id, MilestoneAllocation::new(date));
        state.inserted.insert(NodeId::Milestone(id));

        self.propagate_to_dependents(NodeId::Milestone(id), state, dependents_map, protect_node)?;

        Ok(())
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
