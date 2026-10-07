// modules{{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::rule_set::RuleSet,
    planner::{graph, plan_entity::PlanItem, plan_update::PlanUpdate},
    traits::{entity::Entity, patch::Patchable},
    types::{
        affinity::Affinity,
        data_block::DataBlock,
        dependency::{DependencyNode, PlanNode},
        error::{PlinkoError, PlinkoResult},
        milestone::Milestone,
        parent::Parent,
        plan_buffer::PlanBuffer,
        short_id::ShortId,
        tag::Tag,
        task::{Assignee, Task, Worker},
        task_group::TaskGroup,
        user::{PlanUserData, PlanUserGroupData},
        workload::Workload,
    },
};
// }}}

pub struct NewPlan {
    pub id: ShortId,
    pub name: String,
    pub start_date: NaiveDate,
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    id: ShortId,
    name: String,
    pub(super) tasks: HashMap<Uuid, Task>,
    pub(super) task_groups: HashMap<Uuid, TaskGroup>,
    pub(super) milestones: HashMap<Uuid, Milestone>,
    pub(super) plan_buffers: HashMap<Uuid, PlanBuffer>,
    pub(super) users: HashMap<Uuid, PlanUserData>,
    pub(super) user_groups: HashMap<Uuid, PlanUserGroupData>,
    pub(super) tags: HashMap<Uuid, Tag>,
    pub(super) affinities: HashMap<Uuid, Affinity>,
    default_availability: RuleSet<Workload>,
    scheduler_data: PlanSchedulerData,
}

// impl Plan{{{
impl From<NewPlan> for Plan {
    fn from(new: NewPlan) -> Self {
        Self {
            id: new.id,
            name: new.name,
            tasks: HashMap::new(),
            task_groups: HashMap::new(),
            milestones: HashMap::new(),
            plan_buffers: HashMap::new(),
            users: HashMap::new(),
            user_groups: HashMap::new(),
            tags: HashMap::new(),
            affinities: HashMap::new(),
            default_availability: RuleSet::default(),
            scheduler_data: PlanSchedulerData::new(new.start_date),
        }
    }
}

impl Plan {
    pub fn id(&self) -> &ShortId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tasks(&self) -> &HashMap<Uuid, Task> {
        &self.tasks
    }

    pub fn task_groups(&self) -> &HashMap<Uuid, TaskGroup> {
        &self.task_groups
    }

    pub fn plan_buffers(&self) -> &HashMap<Uuid, PlanBuffer> {
        &self.plan_buffers
    }

    pub fn milestones(&self) -> &HashMap<Uuid, Milestone> {
        &self.milestones
    }

    pub fn users(&self) -> &HashMap<Uuid, PlanUserData> {
        &self.users
    }

    pub fn user_groups(&self) -> &HashMap<Uuid, PlanUserGroupData> {
        &self.user_groups
    }

    pub fn affinities(&self) -> &HashMap<Uuid, Affinity> {
        &self.affinities
    }

    pub fn tags(&self) -> &HashMap<Uuid, Tag> {
        &self.tags
    }

    /// Availability of placeholder users
    pub fn default_availability(&self) -> &RuleSet<Workload> {
        &self.default_availability
    }

    pub fn scheduler_data(&self) -> &PlanSchedulerData {
        &self.scheduler_data
    }

    /// Items that no dependency chain connects to the plan start, so cannot be scheduled
    pub fn unscheduled(&self) -> HashSet<PlanNode> {
        graph::unscheduled(self)
    }

    /// Whether every affinity linked to the task allows `day`, `None` if there is no such task
    pub fn task_allows_day(&self, task: &Uuid, day: NaiveDate) -> Option<bool> {
        let task = self.tasks.get(task)?;
        let mut affinities = task
            .affinities()
            .iter()
            .filter_map(|a| self.affinities.get(a));
        Some(affinities.all(|a| a.allows(day)))
    }

    pub fn ensure_node_exists(&self, node: &PlanNode) -> PlinkoResult<()> {
        let (id, exists) = match node {
            PlanNode::Task(id) => (id, self.tasks.contains_key(id)),
            PlanNode::Milestone(id) => (id, self.milestones.contains_key(id)),
            PlanNode::PlanBuffer(id) => (id, self.plan_buffers.contains_key(id)),
            PlanNode::TaskGroup(id) => (id, self.task_groups.contains_key(id)),
        };
        exists
            .then_some(())
            .ok_or(PlinkoError::ItemDoesNotExist(*id))
    }

    fn ensure_worker_exists(&self, worker: &Worker) -> PlinkoResult<()> {
        let (id, exists) = match worker {
            Worker::Tag(id) => (id, self.tags.contains_key(id)),
            Worker::User(id) => (id, self.users.contains_key(id)),
            Worker::UserGroup(id) => (id, self.user_groups.contains_key(id)),
            Worker::Placeholder => return Ok(()),
        };
        exists
            .then_some(())
            .ok_or(PlinkoError::ItemDoesNotExist(*id))
    }

    /// The parent must be an existing group and must not be the item itself or one of its
    /// descendants.
    fn validate_parent(&self, id: Uuid, parent: Parent) -> PlinkoResult<()> {
        let mut next = parent;
        while let Parent::Node(group) = next {
            if group == id {
                return Err(PlinkoError::GroupCycle(id));
            }
            next = self
                .task_groups
                .get(&group)
                .ok_or(PlinkoError::ItemDoesNotExist(group))?
                .parent();
        }
        Ok(())
    }

    /// Checks everything an item refers to exists and that it introduces no dependency cycle
    pub(super) fn validate_item<T: PlanItem>(&self, id: Uuid) -> PlinkoResult<()> {
        let item = T::entities(self)
            .get(&id)
            .ok_or(PlinkoError::ItemDoesNotExist(id))?;
        self.validate_parent(id, item.parent())?;
        for dep in item.dependencies() {
            if let DependencyNode::Node(node) = &dep.node {
                self.ensure_node_exists(node)?;
            }
        }
        item.workers()
            .iter()
            .try_for_each(|w| self.ensure_worker_exists(w))?;
        let asked = item.workers();
        if item.recorded_workers().iter().any(|w| !asked.contains(w)) {
            return Err(PlinkoError::UnknownWorkerSlot(id));
        }
        for affinity in item.affinities() {
            if !self.affinities.contains_key(&affinity) {
                return Err(PlinkoError::ItemDoesNotExist(affinity));
            }
        }
        match graph::has_cycle_through(self, T::node(id)) {
            true => Err(PlinkoError::DependencyCycle(id)),
            false => Ok(()),
        }
    }

    fn node_depended_on<T: PlanItem>(&self, node: PlanNode) -> bool {
        let target = DependencyNode::Node(node);
        T::entities(self)
            .values()
            .any(|i| i.dependencies().iter().any(|d| d.node == target))
    }

    fn has_members<T: PlanItem>(&self, group: Uuid) -> bool {
        let parent = Parent::Node(group);
        T::entities(self).values().any(|i| i.parent() == parent)
    }

    /// Whether anything still refers to the node: a dependency, a group member or the scheduler
    pub(super) fn node_in_use(&self, node: PlanNode) -> bool {
        let is_group_with_members = match node {
            PlanNode::TaskGroup(id) => {
                self.has_members::<Task>(id)
                    || self.has_members::<Milestone>(id)
                    || self.has_members::<PlanBuffer>(id)
                    || self.has_members::<TaskGroup>(id)
            }
            _ => false,
        };
        is_group_with_members
            || self.scheduler_data.scheduler_target == Some(node)
            || self.node_depended_on::<Task>(node)
            || self.node_depended_on::<Milestone>(node)
            || self.node_depended_on::<PlanBuffer>(node)
            || self.node_depended_on::<TaskGroup>(node)
    }

    /// Whether a task records the person as having done work, which keeps them from being removed
    pub(crate) fn recorded_in_use(&self, who: Assignee) -> bool {
        let recorded = |t: &Task| t.actual_workers().values().any(|a| *a == who);
        self.tasks.values().any(recorded)
    }

    pub(super) fn worker_in_use(&self, worker: Worker) -> bool {
        self.tasks
            .values()
            .any(|t| t.allocations().contains_key(&worker))
    }

    pub fn apply_update(&mut self, u: PlanUpdate) -> PlinkoResult<()> {
        match u {
            PlanUpdate::Task(update) => Task::apply_crud(self, update),
            PlanUpdate::TaskGroup(update) => TaskGroup::apply_crud(self, update),
            PlanUpdate::Milestone(update) => Milestone::apply_crud(self, update),
            PlanUpdate::PlanBuffer(update) => PlanBuffer::apply_crud(self, update),
            PlanUpdate::User(update) => PlanUserData::apply_crud(self, update),
            PlanUpdate::UserGroup(update) => PlanUserGroupData::apply_crud(self, update),
            PlanUpdate::Tag(update) => Tag::apply_crud(self, update),
            PlanUpdate::Affinity(update) => Affinity::apply_crud(self, update),
            PlanUpdate::AffinityRules(id, update) => self
                .affinities
                .get_mut(&id)
                .ok_or(PlinkoError::ItemDoesNotExist(id))?
                .apply_rule_update(update),
            PlanUpdate::Data(node, update) => {
                let data = match node {
                    PlanNode::Task(id) => self.tasks.get_mut(&id).map(Task::data_mut),
                    PlanNode::Milestone(id) => {
                        self.milestones.get_mut(&id).map(Milestone::data_mut)
                    }
                    _ => return Err("only tasks and milestones hold data blocks".into()),
                };
                let data = data.ok_or_else(|| PlinkoError::ItemDoesNotExist(node.id()))?;
                DataBlock::apply_crud(data, update)
            }
            PlanUpdate::Availability(update) => self.default_availability.apply_update(update),
            PlanUpdate::SchedulerData(patch) => {
                if let Some(Some(node)) = &patch.scheduler_target {
                    self.ensure_node_exists(node)?;
                }
                self.scheduler_data.apply_patch(patch)
            }
        }
    }
}
// }}}

#[derive(Debug, PartialEq)]
pub struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: Option<PlanNode>,
}

// impl PlanSchedulerData{{{
impl PlanSchedulerData {
    fn new(start_date: NaiveDate) -> Self {
        Self {
            start_date,
            scheduler_target: None,
        }
    }

    pub fn start_date(&self) -> &NaiveDate {
        &self.start_date
    }

    pub fn scheduler_target(&self) -> &Option<PlanNode> {
        &self.scheduler_target
    }
}

impl Patchable<PlanSchedulerDataPatch> for PlanSchedulerData {
    fn apply_patch(&mut self, patch: PlanSchedulerDataPatch) -> PlinkoResult<()> {
        if let Some(s) = patch.start_date {
            self.start_date = s;
        }
        if let Some(t) = patch.scheduler_target {
            self.scheduler_target = t
        }
        Ok(())
    }
}
// }}}

#[derive(Debug)]
pub struct PlanSchedulerDataPatch {
    pub start_date: Option<NaiveDate>,
    pub scheduler_target: Option<Option<PlanNode>>,
}
