use crate::data::allocation::WorkSegment;
use crate::data::constraint::ConstraintKind;
use crate::data::ids::TagId;
use crate::data::task::{TaskStatus, WorkerSlot};
use crate::data::{Dependency, MilestoneId, NodeId, Plan, TaskId, UserId, constraint};
use chrono::{Datelike, NaiveDate};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

type NodeChain = Vec<NodeId>;

const EPSILON: f32 = 1e-6;

const MAX_FILL_DAYS: i64 = 3_650; // ~10 years

#[derive(Debug, Clone)]
pub enum SchedulerError {
    EmptyChain,
    MissingTaskAffinity {
        task_name: String,
        required_tags: HashSet<TagId>,
    },
    NoPathsToNode(NodeId),
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
            SchedulerError::DisconnectedNode(node_id) => {
                write!(f, "node {node_id:?} has no path back to PlanStart")
            }
        }
    }
}

struct SchedulerState {
    capacity: HashMap<(UserId, NaiveDate), f32>,
    allocation: PlanAllocation,
    inserted: HashSet<NodeId>,
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

impl Plan {
    pub fn compute_time_optimised_plan(&mut self) -> Result<(), SchedulerError> {
        let today = chrono::Local::now().date_naive();

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

        self.all_tasks_completable()?;
        self.check_all_nodes_connected()?;
        let dependents_map = self.build_dependents_map();
        let mut state = SchedulerState::new(today);
        self.pre_insert_anchored_tasks(&mut state);

        let time_constrained = self.get_time_constrained_nodes();
        for node in time_constrained {
            let list = self.get_priority_sorted_task_list_to_node(node)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, None)?;
                }
            }
        }

        let target = self.scheduler_target;
        if !matches!(target, NodeId::PlanStart) {
            let list = self.get_priority_sorted_task_list_to_node(target)?;
            for id in list {
                if !state.inserted.contains(&id) {
                    self.insert_node(id, &mut state, &dependents_map, Some(target))?;
                }
            }
        }

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

    fn hours_remaining(&self, state: &mut SchedulerState, user_id: UserId, date: NaiveDate) -> f32 {
        *state
            .capacity
            .entry((user_id, date))
            .or_insert_with(|| self.hours_available(&user_id, date))
    }

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
            let start_after = match dep.id {
                NodeId::PlanStart | NodeId::Milestone(_) => pred_end + chrono::Duration::days(lag),
                NodeId::Task(_) => pred_end + chrono::Duration::days(lag + 1),
            };
            earliest = earliest.max(start_after);
        }

        if let NodeId::Task(id) = node_id
            && let Some(asd) = self.tasks.get(&id).and_then(|t| t.actual_start_date)
        {
            earliest = earliest.max(asd);
        }

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
                    state.allocation.constraint_violations.insert(
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
                    state.allocation.constraint_violations.insert(
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

        let mut slot_allocations: Vec<SlotAllocation> = Vec::new();
        let mut task_start: Option<NaiveDate> = None;
        let mut task_end: Option<NaiveDate> = None;

        let task_duration = task.duration_days_target;

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

        let min_end = if task_duration > 0.0 {
            self.advance_working_days(start_date, task_duration.ceil() as u32)
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
                    state.allocation.constraint_violations.insert(
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
                    state.allocation.constraint_violations.insert(
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
            NodeId::Task(tid) => state.allocation.tasks.get(&tid).map(|a| a.start_date),
            NodeId::Milestone(mid) => state.allocation.milestones.get(&mid).map(|a| a.date),
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
            return Ok(()); // Never move the protected node
        }

        if let NodeId::Task(tid) = node_id
            && self
                .tasks
                .get(&tid)
                .map(|t| t.status != TaskStatus::NotStarted)
                .unwrap_or(false)
        {
            return Ok(());
        }

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

    fn calculate_path_duration(&self, path: &NodeChain) -> f32 {
        let mut total_days = 0.0;

        for i in 0..path.len() {
            let current_node = path[i];

            match current_node {
                NodeId::Task(id) => {
                    if let Some(task) = self.tasks.get(&id) {
                        total_days += task.effective_duration_days();
                    }
                }
                NodeId::Milestone(_) | NodeId::PlanStart => {}
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
