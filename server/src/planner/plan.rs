// modules{{{
use crate::helpers::double_option;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::rule_set::RuleSet,
    availability::workload::Workload,
    common::{
        error::{PlinkoError, PlinkoResult},
        short_id::ShortId,
    },
    planner::items::{
        dependency::PlanNode,
        milestone::Milestone,
        parent::Parent,
        plan_buffer::PlanBuffer,
        plan_delay::PlanDelay,
        task::{Assignee, Task, Worker},
        task_group::TaskGroup,
    },
    planner::resources::{
        affinity::Affinity,
        data_block::DataBlock,
        tag::Tag,
        target::Target,
        user::{PlanUserData, PlanUserGroupData},
    },
    planner::{graph, plan_entity::PlanItem, plan_update::PlanUpdate},
    traits::{entity::Entity, patch::Patchable},
};
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    id: ShortId,
    name: String,
    pub(super) tasks: HashMap<Uuid, Task>,
    pub(super) task_groups: HashMap<Uuid, TaskGroup>,
    pub(super) milestones: HashMap<Uuid, Milestone>,
    pub(super) plan_buffers: HashMap<Uuid, PlanBuffer>,
    pub(super) plan_delays: HashMap<Uuid, PlanDelay>,
    pub(super) users: HashMap<Uuid, PlanUserData>,
    pub(super) user_groups: HashMap<Uuid, PlanUserGroupData>,
    pub(super) tags: HashMap<Uuid, Tag>,
    pub(super) affinities: HashMap<Uuid, Affinity>,
    pub(super) targets: HashMap<Uuid, Target>,
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
            plan_delays: HashMap::new(),
            users: HashMap::new(),
            user_groups: HashMap::new(),
            tags: HashMap::new(),
            affinities: HashMap::new(),
            targets: HashMap::new(),
            default_availability: RuleSet::default(),
            scheduler_data: PlanSchedulerData::default(),
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

    pub fn plan_delays(&self) -> &HashMap<Uuid, PlanDelay> {
        &self.plan_delays
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

    /// Nodes the scheduler brings forward, none means the whole plan end
    pub fn targets(&self) -> &HashMap<Uuid, Target> {
        &self.targets
    }

    pub fn tags(&self) -> &HashMap<Uuid, Tag> {
        &self.tags
    }

    /// Availability of placeholder users
    pub fn default_availability(&self) -> &RuleSet<Workload> {
        &self.default_availability
    }

    /// Gives the plan's placeholders these rules, as a new plan starts from the tenant's default
    pub(crate) fn set_default_availability(&mut self, rules: RuleSet<Workload>) {
        self.default_availability = rules;
    }

    pub fn scheduler_data(&self) -> &PlanSchedulerData {
        &self.scheduler_data
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
            PlanNode::PlanDelay(id) => (id, self.plan_delays.contains_key(id)),
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
        for node in item.dependencies() {
            self.ensure_node_exists(node)?;
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
        let target = node;
        T::entities(self)
            .values()
            .any(|i| i.dependencies().contains(&target))
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
                    || self.has_members::<PlanDelay>(id)
                    || self.has_members::<TaskGroup>(id)
            }
            _ => false,
        };
        is_group_with_members
            || self.targets.values().any(|t| t.node() == node)
            || self.scheduler_data.default_start == Some(node)
            || self.node_depended_on::<Task>(node)
            || self.node_depended_on::<Milestone>(node)
            || self.node_depended_on::<PlanBuffer>(node)
            || self.node_depended_on::<PlanDelay>(node)
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

    /// The updates that cut the dependencies down to what is needed after `update`, which are
    /// the ones it made redundant, see `graph::simplified`
    pub fn simplifications(&self, update: &PlanUpdate) -> Vec<PlanUpdate> {
        let Some(touched) = update.rewires() else {
            return Vec::new();
        };
        graph::simplified(self, touched)
            .into_iter()
            .map(|(node, kept)| PlanUpdate::set_dependencies(node, kept))
            .collect()
    }

    pub fn apply_update(&mut self, u: PlanUpdate) -> PlinkoResult<()> {
        match u {
            PlanUpdate::Task(update) => Task::apply_crud(self, update),
            PlanUpdate::TaskGroup(update) => TaskGroup::apply_crud(self, update),
            PlanUpdate::Milestone(update) => Milestone::apply_crud(self, update),
            PlanUpdate::PlanBuffer(update) => PlanBuffer::apply_crud(self, update),
            PlanUpdate::PlanDelay(update) => PlanDelay::apply_crud(self, update),
            PlanUpdate::User(update) => PlanUserData::apply_crud(self, update),
            PlanUpdate::UserGroup(update) => PlanUserGroupData::apply_crud(self, update),
            PlanUpdate::Tag(update) => Tag::apply_crud(self, update),
            PlanUpdate::Target(update) => Target::apply_crud(self, update),
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
                    PlanNode::TaskGroup(id) => {
                        self.task_groups.get_mut(&id).map(TaskGroup::data_mut)
                    }
                    PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => {
                        return Err("buffers and delays hold no data blocks".into());
                    }
                };
                let data = data.ok_or_else(|| PlinkoError::ItemDoesNotExist(node.id()))?;
                DataBlock::apply_crud(data, update)
            }
            PlanUpdate::Describe(node, text) => {
                let missing = || PlinkoError::ItemDoesNotExist(node.id());
                // an empty description is no description
                let text = text.filter(|t| !t.is_empty());
                match node {
                    PlanNode::Task(id) => {
                        self.tasks
                            .get_mut(&id)
                            .ok_or_else(missing)?
                            .set_description(text);
                    }
                    PlanNode::Milestone(id) => {
                        self.milestones
                            .get_mut(&id)
                            .ok_or_else(missing)?
                            .set_description(text);
                    }
                    PlanNode::TaskGroup(id) => {
                        self.task_groups
                            .get_mut(&id)
                            .ok_or_else(missing)?
                            .set_description(text);
                    }
                    PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => {
                        return Err("buffers and delays have no description".into());
                    }
                }
                Ok(())
            }
            PlanUpdate::Availability(update) => self.default_availability.apply_update(update),
            PlanUpdate::SchedulerData(patch) => {
                if let Some(Some(node)) = &patch.default_start {
                    self.ensure_node_exists(node)?;
                }
                self.scheduler_data.apply_patch(patch)
            }
        }
    }

    pub fn get_node_dependencies(&self, node: &Uuid) -> PlinkoResult<&[PlanNode]> {
        let node = self
            .node(node)
            .ok_or(PlinkoError::ItemDoesNotExist(*node))?;
        Ok(node.dependencies())
    }

    pub fn get_node_parent(&self, node: &Uuid) -> PlinkoResult<Parent> {
        let node = self
            .node(node)
            .ok_or(PlinkoError::ItemDoesNotExist(*node))?;
        Ok(node.parent())
    }
}
// }}}

// Plan supporting types {{{
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanSchedulerData {
    priority: i32,
    /// What new top level items depend on to begin with, set by clients for convenience
    #[serde(default)]
    default_start: Option<PlanNode>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewPlan {
    pub id: ShortId,
    pub name: String,
}

// impl PlanSchedulerData{{{
impl PlanSchedulerData {
    pub fn default_start(&self) -> Option<PlanNode> {
        self.default_start
    }

    /// Added to each target's priority, positive treats the plan as further behind
    pub fn priority(&self) -> i32 {
        self.priority
    }
}

impl Patchable<PlanSchedulerDataPatch> for PlanSchedulerData {
    fn apply_patch(&mut self, patch: PlanSchedulerDataPatch) -> PlinkoResult<()> {
        if let Some(p) = patch.priority {
            self.priority = p;
        }
        if let Some(node) = patch.default_start {
            self.default_start = node;
        }
        Ok(())
    }
}
// }}}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanSchedulerDataPatch {
    pub priority: Option<i32>,
    /// The node new top level items depend on, `Some(None)` to have none
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub default_start: Option<Option<PlanNode>>,
}
// }}}
