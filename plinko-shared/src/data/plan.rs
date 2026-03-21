//! The [`Plan`] aggregate root and its [`DependencyError`] type.

use std::collections::{HashMap, HashSet};

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

pub type NodeChain = Vec<NodeId>;

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
            let ts = self
                .node_allocations
                .tasks
                .entry(id)
                .or_insert_with(TaskState::not_started);
            match &mut ts.allocation {
                TaskAllocation::Fixed { start_date, .. } => *start_date = date,
                TaskAllocation::Dynamic { .. } => {
                    let end = ts.allocation.end_date();
                    ts.allocation = TaskAllocation::Fixed {
                        start_date: date,
                        end_date: end,
                        corrected_end_date: None,
                        time_allocation: vec![],
                    };
                }
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
        let (existing_end, existing_time_alloc) = match self
            .node_allocations
            .tasks
            .get(&id)
            .map(|ts| &ts.allocation)
        {
            Some(TaskAllocation::Dynamic {
                scheduled_end_date,
                time_allocation,
                ..
            }) => (*scheduled_end_date, time_allocation.clone()),
            Some(TaskAllocation::Fixed {
                end_date,
                time_allocation,
                ..
            }) => (*end_date, time_allocation.clone()),
            None => (today, vec![]),
        };
        let ts = self
            .node_allocations
            .tasks
            .entry(id)
            .or_insert_with(TaskState::not_started);
        ts.status = Status::InProgress;
        ts.allocation = TaskAllocation::Fixed {
            start_date: today,
            end_date: existing_end.max(today),
            corrected_end_date: None,
            time_allocation: existing_time_alloc,
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

    pub fn get_dependencies(&self, node_id: &NodeId) -> Vec<Dependency> {
        match node_id {
            NodeId::Task(task_id) => self
                .tasks
                .get(task_id)
                .map(|t| t.dependencies.clone())
                .unwrap_or_default(),
            NodeId::Milestone(milestone_id) => self
                .milestones
                .get(milestone_id)
                .map(|m| m.dependencies.clone())
                .unwrap_or_default(),
            NodeId::PlanStart => vec![],
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

    pub fn check_all_nodes_connected(&self) -> Result<(), SchedulerError> {
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

    pub fn get_priority_sorted_task_list_to_ends(&self) -> Result<Vec<NodeId>, SchedulerError> {
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

    pub fn get_all_paths_to_node(&self, target: NodeId) -> Result<Vec<NodeChain>, SchedulerError> {
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

    // ── Private helpers ───────────────────────────────────────────────────────

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
        let deps = self.get_dependencies(node_id);
        deps.iter().try_fold(vec![], |mut acc, dependency| {
            let mut new_chain = current_chain.clone();
            new_chain.push(dependency.id);
            acc.extend(self.get_all_paths_to_root(new_chain)?);
            Ok(acc)
        })
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
}
// }}}
