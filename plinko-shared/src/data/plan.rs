//! The [`Plan`] aggregate root and its [`DependencyError`] type.

use std::collections::{BinaryHeap, HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::data::allocation::{NodeAllocations, Status, TaskAllocation, TaskState};
use crate::data::dependency::Dependency;
use crate::data::ids::{NodeId, TagId};
use crate::data::scheduler::SchedulerError;
use crate::data::{
    CalendarOverrides, Milestone, MilestoneId, Task, TaskId, User, UserId, WorkSchedule, WorkerSlot,
};
use crate::data::{Tag, UserData};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyError {
    Cycle,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub name: String,
    pub users_data: HashMap<UserId, UserData>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    pub tasks: HashMap<TaskId, Task>,
    pub milestones: HashMap<MilestoneId, Milestone>,
    pub start_date: NaiveDate,
    pub default_schedule: WorkSchedule,
    pub calendar: CalendarOverrides,
    pub user_calendar_overrides: HashMap<UserId, CalendarOverrides>,
    pub scheduler_target: NodeId,
    #[serde(default)]
    pub node_allocations: NodeAllocations,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Plan {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            users_data: HashMap::new(),
            tags: Vec::new(),
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            start_date: chrono::Local::now().date_naive(),
            default_schedule: WorkSchedule::default(),
            calendar: CalendarOverrides::default(),
            user_calendar_overrides: HashMap::new(),
            scheduler_target: NodeId::PlanStart,
            node_allocations: NodeAllocations::default(),
        }
    }

    // ── User accessors ────────────────────────────────────────────────────────

    pub fn user(&self, id: &UserId) -> Option<&User> {
        self.users_data.get(id).map(|ud| &ud.user)
    }

    pub fn user_mut(&mut self, id: &UserId) -> Option<&mut User> {
        self.users_data.get_mut(id).map(|ud| &mut ud.user)
    }

    pub fn schedule_for(&self, user_id: &UserId) -> &WorkSchedule {
        self.users_data
            .get(user_id)
            .and_then(|ud| ud.schedule.as_ref())
            .unwrap_or(&self.default_schedule)
    }

    pub fn set_user_schedule(&mut self, user_id: UserId, schedule: WorkSchedule) {
        if let Some(ud) = self.users_data.get_mut(&user_id) {
            ud.schedule = Some(schedule);
        }
        self.node_allocations.invalidate();
    }

    pub fn clear_user_schedule(&mut self, user_id: &UserId) {
        if let Some(ud) = self.users_data.get_mut(user_id) {
            ud.schedule = None;
        }
        self.node_allocations.invalidate();
    }

    /// Effective hours available for a user on a specific date.
    /// Resolution: user calendar → plan calendar → user schedule → plan schedule.
    pub fn hours_available(&self, user_id: &UserId, date: NaiveDate) -> f32 {
        if let Some(h) = self
            .user_calendar_overrides
            .get(user_id)
            .and_then(|c| c.get(date))
        {
            return h;
        }
        if let Some(h) = self.calendar.get(date) {
            return h;
        }
        let weekday = crate::data::schedule::chrono_to_weekday(date.weekday());
        self.schedule_for(user_id).hours_on(weekday)
    }

    pub fn users_with_tags<'a>(&'a self, tag_ids: &[TagId]) -> impl Iterator<Item = &'a User> {
        let tag_set: HashSet<&TagId> = tag_ids.iter().collect();
        self.users_data
            .values()
            .map(|ud| &ud.user)
            .filter(move |user| tag_set.iter().all(|id| user.tags.contains(id)))
    }

    // ── CRUD ─────────────────────────────────────────────────────────────────

    pub fn add_task(&mut self, task: Task) -> TaskId {
        let id = task.id;
        self.tasks.insert(id, task);
        self.node_allocations.invalidate();
        id
    }

    pub fn add_milestone(&mut self, milestone: Milestone) -> MilestoneId {
        let id = milestone.id;
        self.milestones.insert(id, milestone);
        self.node_allocations.invalidate();
        id
    }

    pub fn add_user(&mut self, user: User) -> UserId {
        let id = user.id;
        self.users_data.insert(id, UserData::new(user));
        id
    }

    // ── Tag registry ──────────────────────────────────────────────────────────

    pub fn add_tag(&mut self, name: impl Into<String>) -> Option<TagId> {
        let name = name.into();
        if self.tags.iter().any(|t| t.name == name) {
            return None;
        }
        let tag = Tag::new(name);
        let id = tag.id;
        self.tags.push(tag);
        Some(id)
    }

    pub fn rename_tag(&mut self, id: &TagId, new_name: &str) -> bool {
        if self.tags.iter().any(|t| &t.id != id && t.name == new_name) {
            return false;
        }
        match self.tags.iter_mut().find(|t| &t.id == id) {
            Some(tag) => {
                tag.name = new_name.to_string();
                true
            }
            None => false,
        }
    }

    pub fn remove_tag(&mut self, id: &TagId) {
        self.tags.retain(|t| &t.id != id);
        for ud in self.users_data.values_mut() {
            ud.user.tags.remove(id);
        }
        for task in self.tasks.values_mut() {
            for slot in task.workers.iter_mut() {
                if let WorkerSlot::Placeholder { required_tags, .. } = slot {
                    required_tags.remove(id);
                }
            }
        }
        self.node_allocations.invalidate();
    }

    pub fn move_tag(&mut self, id: &TagId, new_index: usize) -> bool {
        let pos = match self.tags.iter().position(|t| &t.id == id) {
            Some(p) => p,
            None => return false,
        };
        let tag = self.tags.remove(pos);
        let insert_at = new_index.min(self.tags.len());
        self.tags.insert(insert_at, tag);
        true
    }

    // ── Dependency management ─────────────────────────────────────────────────

    pub fn add_task_dependency(
        &mut self,
        task_id: TaskId,
        dep: Dependency,
    ) -> Result<(), DependencyError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(DependencyError::NotFound);
        }
        if self.has_dependency_path(dep.id, NodeId::Task(task_id)) {
            return Err(DependencyError::Cycle);
        }
        let task = self.tasks.get_mut(&task_id).unwrap();
        if let Some(existing) = task.dependencies.iter_mut().find(|d| d.id == dep.id) {
            existing.lag_days = dep.lag_days;
        } else {
            task.dependencies.push(dep);
        }
        self.node_allocations.invalidate();
        Ok(())
    }

    pub fn add_milestone_dependency(
        &mut self,
        milestone_id: MilestoneId,
        dep: Dependency,
    ) -> Result<(), DependencyError> {
        if !self.milestones.contains_key(&milestone_id) {
            return Err(DependencyError::NotFound);
        }
        if self.has_dependency_path(dep.id, NodeId::Milestone(milestone_id)) {
            return Err(DependencyError::Cycle);
        }
        let milestone = self.milestones.get_mut(&milestone_id).unwrap();
        if let Some(existing) = milestone.dependencies.iter_mut().find(|d| d.id == dep.id) {
            existing.lag_days = dep.lag_days;
        } else {
            milestone.dependencies.push(dep);
        }
        self.node_allocations.invalidate();
        Ok(())
    }

    pub fn has_dependency_path(&self, start: NodeId, target: NodeId) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            match current {
                NodeId::Task(id) => {
                    if let Some(task) = self.tasks.get(&id) {
                        stack.extend(task.dependencies.iter().map(|d| d.id));
                    }
                }
                NodeId::Milestone(id) => {
                    if let Some(milestone) = self.milestones.get(&id) {
                        stack.extend(milestone.dependencies.iter().map(|d| d.id));
                    }
                }
                NodeId::PlanStart => {}
            }
        }
        false
    }

    // ── Task status helpers ───────────────────────────────────────────────────

    pub fn task_status(&self, id: &TaskId) -> Status {
        self.node_allocations
            .tasks
            .get(id)
            .map(|ts| ts.status)
            .unwrap_or(Status::NotStarted)
    }

    pub fn task_actual_start(&self, id: &TaskId) -> Option<NaiveDate> {
        // Prefer the field stored on the task itself (set when task is started).
        if let Some(task) = self.tasks.get(id) {
            if let Some(d) = task.actual_start {
                return Some(d);
            }
        }
        // Fall back to Fixed allocation start_date for legacy plans.
        match self.node_allocations.tasks.get(id)?.allocation {
            TaskAllocation::Fixed { start_date, .. } => Some(start_date),
            TaskAllocation::Dynamic { .. } => None,
        }
    }

    pub fn task_actual_end(&self, id: &TaskId) -> Option<NaiveDate> {
        match self.node_allocations.tasks.get(id)?.allocation {
            TaskAllocation::Fixed {
                end_date,
                corrected_end_date,
                ..
            } => Some(corrected_end_date.unwrap_or(end_date)),
            TaskAllocation::Dynamic { .. } => None,
        }
    }

    pub fn set_task_status(&mut self, id: TaskId, status: Status) {
        self.node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started)
            .status = status;
    }

    pub fn set_task_actual_start(&mut self, id: TaskId, date: Option<NaiveDate>) {
        if let Some(date) = date {
            // Store on the task itself so the scheduler can read it.
            if let Some(task) = self.tasks.get_mut(&id) {
                task.actual_start = Some(date);
            }
            // Also update legacy Fixed allocation if present.
            let ts = self
                .node_allocations
                .tasks
                .entry(id)
                .or_insert_with(TaskState::not_started);
            match &mut ts.allocation {
                TaskAllocation::Fixed { start_date, .. } => *start_date = date,
                TaskAllocation::Dynamic { .. } => {}
            }
        }
    }

    pub fn set_task_actual_end(&mut self, id: TaskId, date: Option<NaiveDate>) {
        if let Some(date) = date {
            let ts = self
                .node_allocations
                .tasks
                .entry(id)
                .or_insert_with(TaskState::not_started);
            match &mut ts.allocation {
                TaskAllocation::Fixed {
                    corrected_end_date, ..
                } => {
                    *corrected_end_date = Some(date);
                }
                TaskAllocation::Dynamic { .. } => {
                    let start = ts.allocation.start_date();
                    ts.allocation = TaskAllocation::Fixed {
                        start_date: start,
                        end_date: date,
                        corrected_end_date: None,
                        time_allocation: vec![],
                    };
                }
            }
        }
    }

    // ── Task lifecycle ────────────────────────────────────────────────────────

    pub fn start_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let today = chrono::Local::now().date_naive();
        // Record actual_start on the task only if not already set.
        if let Some(task) = self.tasks.get_mut(&id) {
            if task.actual_start.is_none() {
                task.actual_start = Some(today);
            }
        }
        let actual_start = self.tasks[&id].actual_start.unwrap_or(today);
        // Set status to InProgress. Use a Fixed allocation so the status
        // survives invalidate() calls (which purge Dynamic scheduler output).
        // The scheduler detects InProgress Fixed allocations and reschedules
        // them dynamically from actual_start.
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::InProgress;
        ts.allocation = TaskAllocation::Fixed {
            start_date: actual_start,
            end_date: actual_start,
            corrected_end_date: None,
            time_allocation: vec![],
        };
    }

    pub fn complete_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let today = chrono::Local::now().date_naive();
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::Complete;
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

    pub fn pause_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::OnHold;
    }

    pub fn resume_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::InProgress;
    }

    pub fn drop_task(&mut self, id: TaskId) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::Dropped;
    }

    /// Remove a task, preserving the dependency chain: every task/milestone that
    /// depended on this task inherits its dependencies (deduplicated), so no gaps
    /// appear in existing chains. The entire operation is atomic — no intermediate
    /// scheduler runs occur.
    pub fn delete_task(&mut self, id: TaskId) -> bool {
        let Some(task) = self.tasks.remove(&id) else {
            return false;
        };
        self.node_allocations.tasks.remove(&id);
        let node = NodeId::Task(id);
        let inherited: Vec<Dependency> = task.dependencies.clone();

        for other_task in self.tasks.values_mut() {
            if other_task.dependencies.iter().any(|d| d.id == node) {
                other_task.dependencies.retain(|d| d.id != node);
                for dep in &inherited {
                    if !other_task.dependencies.iter().any(|d| d.id == dep.id) {
                        other_task.dependencies.push(dep.clone());
                    }
                }
            }
        }
        for milestone in self.milestones.values_mut() {
            if milestone.dependencies.iter().any(|d| d.id == node) {
                milestone.dependencies.retain(|d| d.id != node);
                for dep in &inherited {
                    if !milestone.dependencies.iter().any(|d| d.id == dep.id) {
                        milestone.dependencies.push(dep.clone());
                    }
                }
            }
        }
        if self.scheduler_target == node {
            self.scheduler_target = NodeId::PlanStart;
        }
        true
    }

    /// Remove a milestone, preserving the dependency chain: every task/milestone
    /// that depended on this milestone inherits its dependencies (deduplicated).
    /// The entire operation is atomic — no intermediate scheduler runs occur.
    pub fn delete_milestone(&mut self, id: MilestoneId) -> bool {
        let Some(milestone) = self.milestones.remove(&id) else {
            return false;
        };
        self.node_allocations.milestones.remove(&id);
        let node = NodeId::Milestone(id);
        let inherited: Vec<Dependency> = milestone.dependencies.clone();

        for task in self.tasks.values_mut() {
            if task.dependencies.iter().any(|d| d.id == node) {
                task.dependencies.retain(|d| d.id != node);
                for dep in &inherited {
                    if !task.dependencies.iter().any(|d| d.id == dep.id) {
                        task.dependencies.push(dep.clone());
                    }
                }
            }
        }
        for other_ms in self.milestones.values_mut() {
            if other_ms.dependencies.iter().any(|d| d.id == node) {
                other_ms.dependencies.retain(|d| d.id != node);
                for dep in &inherited {
                    if !other_ms.dependencies.iter().any(|d| d.id == dep.id) {
                        other_ms.dependencies.push(dep.clone());
                    }
                }
            }
        }
        if self.scheduler_target == node {
            self.scheduler_target = NodeId::PlanStart;
        }
        true
    }

    // ── Scheduler helpers ─────────────────────────────────────────────────────

    /// Resolve the start date of any dependency.
    pub fn start_of(&self, dep: NodeId) -> Option<NaiveDate> {
        match dep {
            NodeId::PlanStart => Some(self.start_date),
            NodeId::Task(id) => self
                .node_allocations
                .tasks
                .get(&id)
                .map(|ts| ts.allocation.start_date()),
            NodeId::Milestone(id) => self
                .node_allocations
                .milestones
                .get(&id)
                .map(|ma| ma.date()),
        }
    }

    pub fn get_dependencies(&self, node_id: &NodeId) -> &[Dependency] {
        match node_id {
            NodeId::Task(task_id) => self
                .tasks
                .get(task_id)
                .map(|t| t.dependencies.as_slice())
                .unwrap_or_default(),
            NodeId::Milestone(milestone_id) => self
                .milestones
                .get(milestone_id)
                .map(|m| m.dependencies.as_slice())
                .unwrap_or_default(),
            NodeId::PlanStart => &[],
        }
    }

    pub fn all_tasks_completable(&self) -> Result<(), SchedulerError> {
        use crate::data::task::WorkerSlot;
        let users: Vec<_> = self.users_data.values().map(|ud| &ud.user).collect();
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

    pub fn check_all_nodes_connected(
        &self,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
    ) -> Result<(), SchedulerError> {
        // Forward BFS from PlanStart through the dependents map.
        // Any task/milestone not reached is not connected to PlanStart.
        let mut reachable: HashSet<NodeId> = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(NodeId::PlanStart);
        while let Some(node) = queue.pop_front() {
            if reachable.insert(node)
                && let Some(deps) = dependents_map.get(&node)
            {
                for &d in deps {
                    queue.push_back(d);
                }
            }
        }
        for &id in self.tasks.keys() {
            if !reachable.contains(&NodeId::Task(id)) {
                return Err(SchedulerError::DisconnectedNode(NodeId::Task(id)));
            }
        }
        for &id in self.milestones.keys() {
            if !reachable.contains(&NodeId::Milestone(id)) {
                return Err(SchedulerError::DisconnectedNode(NodeId::Milestone(id)));
            }
        }
        Ok(())
    }

    pub fn build_dependents_map(&self) -> HashMap<NodeId, Vec<NodeId>> {
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

    pub fn get_time_constrained_nodes(&self) -> Vec<NodeId> {
        use crate::data::constraint::ConstraintKind;
        let mut v: Vec<(NodeId, crate::data::DateConstraint)> = self
            .tasks
            .iter()
            .filter_map(|(&id, task)| {
                task.constraint
                    .filter(|c| matches!(c.kind, ConstraintKind::Fixed | ConstraintKind::Latest))
                    .map(|c| (NodeId::Task(id), c))
            })
            .chain(self.milestones.iter().filter_map(|(&id, milestone)| {
                milestone
                    .constraint
                    .filter(|c| matches!(c.kind, ConstraintKind::Fixed | ConstraintKind::Latest))
                    .map(|c| (NodeId::Milestone(id), c))
            }))
            .collect();
        v.sort_by_key(|(_, c)| c.date);
        v.into_iter().map(|(id, _)| id).collect()
    }

    pub fn get_priority_sorted_task_list_to_node(
        &self,
        node_id: NodeId,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
    ) -> Result<Vec<NodeId>, SchedulerError> {
        let ancestors = self.collect_ancestors(node_id);
        if !ancestors.contains(&NodeId::PlanStart) {
            return Err(SchedulerError::NoPathsToNode(node_id));
        }
        Ok(self.topological_critical_path_sort(&ancestors, dependents_map))
    }

    pub fn get_priority_sorted_task_list_to_ends(
        &self,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
    ) -> Result<Vec<NodeId>, SchedulerError> {
        let all_nodes: HashSet<NodeId> = std::iter::once(NodeId::PlanStart)
            .chain(self.tasks.keys().map(|&id| NodeId::Task(id)))
            .chain(self.milestones.keys().map(|&id| NodeId::Milestone(id)))
            .collect();
        Ok(self.topological_critical_path_sort(&all_nodes, dependents_map))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// BFS backward from `target`, collecting all ancestor NodeIds (including `target`).
    fn collect_ancestors(&self, target: NodeId) -> HashSet<NodeId> {
        let mut visited = HashSet::new();
        let mut stack = vec![target];
        while let Some(node) = stack.pop() {
            if visited.insert(node) {
                for dep in self.get_dependencies(&node) {
                    stack.push(dep.id);
                }
            }
        }
        visited
    }

    /// Topological sort (Kahn's algorithm) of the nodes in `subset`, with a
    /// priority queue that processes ready nodes in descending critical-path
    /// order.  This guarantees two properties simultaneously:
    ///   1. Topological validity — a node is only processed once every one of
    ///      its predecessors has been processed (in-degree reaches zero).
    ///   2. Critical-path priority — among nodes whose predecessors are all
    ///      done, the node with the longest critical path is processed first so
    ///      that the scheduler allocates capacity to the most constrained chains
    ///      first.
    ///
    /// The previous approach (plain Kahn + post-sort by crit-path) violated (1)
    /// because a node N could share crit_to with its predecessor and end up
    /// sorted before shorter-path predecessors that N depends on.
    fn topological_critical_path_sort(
        &self,
        subset: &HashSet<NodeId>,
        dependents_map: &HashMap<NodeId, Vec<NodeId>>,
    ) -> Vec<NodeId> {
        // Compute in-degree within the subset.
        let mut in_degree: HashMap<NodeId, usize> = subset.iter().map(|&n| (n, 0usize)).collect();
        for &node in subset {
            for dep in self.get_dependencies(&node) {
                if subset.contains(&dep.id) {
                    *in_degree.get_mut(&node).unwrap() += 1;
                }
            }
        }

        // Forward DP: crit_to[v] = longest (weighted) path from PlanStart to
        // the end of v, counting task durations and dependency lags.
        // We compute crit_to[v] the moment v's in-degree reaches zero (i.e.,
        // all predecessors are already in crit_to).
        let mut crit_to: HashMap<NodeId, f32> = HashMap::new();
        crit_to.insert(NodeId::PlanStart, 0.0);

        // Compute crit_to for all zero-in-degree nodes and seed the heap.
        // BinaryHeap is a max-heap.  We want descending crit_to and, for
        // equal crit_to, ascending NodeId (Task < Milestone) so tasks are
        // scheduled before milestones at the same depth.
        // Key: (scaled_crit as i64, Reverse<NodeId>)
        let mut heap: BinaryHeap<(i64, std::cmp::Reverse<NodeId>)> = BinaryHeap::new();

        for (&node, &deg) in &in_degree {
            if deg == 0 {
                let node_dur = match node {
                    NodeId::Task(id) => self
                        .tasks
                        .get(&id)
                        .map(|t| t.effective_duration_days())
                        .unwrap_or(0.0),
                    _ => 0.0,
                };
                // Zero-in-degree nodes have no predecessors in the subset.
                crit_to.insert(node, node_dur);
                let c = (node_dur * 1_000.0) as i64;
                heap.push((c, std::cmp::Reverse(node)));
            }
        }

        let mut topo_order = Vec::with_capacity(subset.len());

        while let Some((_, std::cmp::Reverse(node))) = heap.pop() {
            topo_order.push(node);

            if let Some(dependents) = dependents_map.get(&node) {
                for &dep in dependents {
                    if let Some(deg) = in_degree.get_mut(&dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            // All predecessors of `dep` are now in crit_to.
                            let dep_dur = match dep {
                                NodeId::Task(id) => self
                                    .tasks
                                    .get(&id)
                                    .map(|t| t.effective_duration_days())
                                    .unwrap_or(0.0),
                                _ => 0.0,
                            };
                            let mut best = 0.0f32;
                            for pred_dep in self.get_dependencies(&dep) {
                                if subset.contains(&pred_dep.id) {
                                    let pred = crit_to.get(&pred_dep.id).copied().unwrap_or(0.0);
                                    best = best.max(pred + pred_dep.lag_days.max(0.0));
                                }
                            }
                            let dep_crit = best + dep_dur;
                            crit_to.insert(dep, dep_crit);
                            let c = (dep_crit * 1_000.0) as i64;
                            heap.push((c, std::cmp::Reverse(dep)));
                        }
                    }
                }
            }
        }

        // Exclude PlanStart from the result.
        topo_order.retain(|n| !matches!(n, NodeId::PlanStart));
        topo_order
    }
}
// }}}
