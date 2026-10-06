// modules{{{
use std::collections::{HashMap, hash_map::Entry};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    planner::{
        graph,
        plan_item::PlanItem,
        plan_update::{
            MilestoneUpdate, PlanBufferUpdate, PlanUpdate, SchedulerDataUpdate, TagUpdate,
            TaskGroupUpdate, TaskUpdate, UserUpdate,
        },
    },
    traits::patch::Patchable,
    types::{
        dependency::{DependencyNode, PlanNode},
        error::{PlinkoError, PlinkoResult},
        milestone::{self, Milestone},
        parent::Parent,
        plan_buffer::PlanBuffer,
        schedule::{CalendarChanges, WeeklySchedule, WeeklySchedules},
        short_id::ShortId,
        tag::Tag,
        task::{self, Task, Worker},
        task_group::TaskGroup,
        user::PlanUserData,
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
    pub(super) tags: HashMap<Uuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
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
            tags: HashMap::new(),
            plan_weekly_schedule: WeeklySchedules::new(WeeklySchedule::new_initial_schedule()),
            plan_calendar_changes: CalendarChanges::new(),
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

    pub fn tags(&self) -> &HashMap<Uuid, Tag> {
        &self.tags
    }

    pub fn weekly_schedules(&self) -> &WeeklySchedules {
        &self.plan_weekly_schedule
    }

    pub fn calendar_changes(&self) -> &CalendarChanges {
        &self.plan_calendar_changes
    }

    pub fn scheduler_data(&self) -> &PlanSchedulerData {
        &self.scheduler_data
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
    fn validate_item<T: PlanItem>(&self, id: Uuid) -> PlinkoResult<()> {
        let item = T::items(self)
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
        match graph::has_cycle_through(self, T::node(id)) {
            true => Err(PlinkoError::DependencyCycle(id)),
            false => Ok(()),
        }
    }

    fn node_depended_on<T: PlanItem>(&self, node: PlanNode) -> bool {
        let target = DependencyNode::Node(node);
        T::items(self)
            .values()
            .any(|i| i.dependencies().iter().any(|d| d.node == target))
    }

    fn has_members<T: PlanItem>(&self, group: Uuid) -> bool {
        let parent = Parent::Node(group);
        T::items(self).values().any(|i| i.parent() == parent)
    }

    /// Whether anything still refers to the node: a dependency, a group member or the scheduler
    fn node_in_use(&self, node: PlanNode) -> bool {
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

    fn worker_in_use(&self, worker: Worker) -> bool {
        self.tasks
            .values()
            .any(|t| t.allocations().contains_key(&worker))
    }

    fn add_checked<T: PlanItem>(&mut self, id: Uuid, item: T) -> PlinkoResult<()> {
        add_item(T::items_mut(self), id, item)?;
        self.validate_item::<T>(id).inspect_err(|_| {
            T::items_mut(self).remove(&id);
        })
    }

    fn update_checked<T: PlanItem + Patchable<P>, P>(
        &mut self,
        id: Uuid,
        patch: P,
    ) -> PlinkoResult<()> {
        let old = T::items(self)
            .get(&id)
            .ok_or(PlinkoError::ItemDoesNotExist(id))?
            .clone();
        update_map_item(T::items_mut(self), id, patch)?;
        self.validate_item::<T>(id).inspect_err(|_| {
            T::items_mut(self).insert(id, old);
        })
    }

    fn remove_checked<T: PlanItem>(&mut self, id: Uuid) -> PlinkoResult<()> {
        if self.node_in_use(T::node(id)) {
            return Err(PlinkoError::ItemInUse(id));
        }
        remove_item(T::items_mut(self), id)
    }

    pub fn apply_update(&mut self, u: PlanUpdate) -> PlinkoResult<()> {
        match u {
            PlanUpdate::Task(update) => self.task_update(update),
            PlanUpdate::TaskGroup(update) => self.task_group_update(update),
            PlanUpdate::User(update) => self.user_update(update),
            PlanUpdate::Milestone(update) => self.milestone_update(update),
            PlanUpdate::PlanBuffer(update) => self.plan_buffer_update(update),
            PlanUpdate::Tag(update) => self.tag_update(update),
            PlanUpdate::SchedulerData(update) => self.scheduler_data_update(update),
        }
    }

    fn task_update(&mut self, update: TaskUpdate) -> PlinkoResult<()> {
        match update {
            TaskUpdate::Add(new) => self.add_checked(new.id, Task::from(new)),
            TaskUpdate::Update(patch) => self.update_checked::<Task, _>(patch.id, patch),
            TaskUpdate::Remove(id) => self.remove_checked::<Task>(id),
        }
    }

    fn task_group_update(&mut self, update: TaskGroupUpdate) -> PlinkoResult<()> {
        match update {
            TaskGroupUpdate::Add(new) => self.add_checked(new.id, TaskGroup::from(new)),
            TaskGroupUpdate::Update(patch) => self.update_checked::<TaskGroup, _>(patch.id, patch),
            TaskGroupUpdate::Remove(id) => self.remove_checked::<TaskGroup>(id),
        }
    }

    fn plan_buffer_update(&mut self, update: PlanBufferUpdate) -> PlinkoResult<()> {
        match update {
            PlanBufferUpdate::Add(new) => self.add_checked(new.id, PlanBuffer::from(new)),
            PlanBufferUpdate::Update(patch) => {
                self.update_checked::<PlanBuffer, _>(patch.id, patch)
            }
            PlanBufferUpdate::Remove(id) => self.remove_checked::<PlanBuffer>(id),
        }
    }

    fn user_update(&mut self, update: UserUpdate) -> PlinkoResult<()> {
        match update {
            UserUpdate::Add(id) => add_item(&mut self.users, id, PlanUserData::default()),
            UserUpdate::Update(user_patch) => {
                update_map_item(&mut self.users, user_patch.id, user_patch)
            }
            UserUpdate::Remove(id) => {
                if self.worker_in_use(Worker::User(id)) {
                    return Err(PlinkoError::ItemInUse(id));
                }
                remove_item(&mut self.users, id)
            }
        }
    }

    fn milestone_update(&mut self, update: MilestoneUpdate) -> PlinkoResult<()> {
        match update {
            MilestoneUpdate::Add(new) => self.add_checked(new.id, Milestone::from(new)),
            MilestoneUpdate::Update(patch) => self.update_checked::<Milestone, _>(patch.id, patch),
            MilestoneUpdate::Remove(id) => self.remove_checked::<Milestone>(id),
        }
    }

    fn tag_update(&mut self, update: TagUpdate) -> PlinkoResult<()> {
        match update {
            TagUpdate::Add(new) => add_item(&mut self.tags, new.id, new.into()),
            TagUpdate::Update(tag_patch) => {
                update_map_item(&mut self.tags, tag_patch.id, tag_patch)
            }
            TagUpdate::Remove(id) => {
                if self.worker_in_use(Worker::Tag(id)) {
                    return Err(PlinkoError::ItemInUse(id));
                }
                remove_item(&mut self.tags, id)
            }
        }
    }

    fn scheduler_data_update(&mut self, update: SchedulerDataUpdate) -> PlinkoResult<()> {
        match update {
            SchedulerDataUpdate::Update(plan_scheduler_data) => {
                if let Some(Some(node)) = &plan_scheduler_data.scheduler_target {
                    self.ensure_node_exists(node)?;
                }
                self.scheduler_data.apply_patch(plan_scheduler_data)
            }
        }
    }
}

fn add_item<T>(map: &mut HashMap<Uuid, T>, id: Uuid, item: T) -> PlinkoResult<()> {
    match map.entry(id) {
        Entry::Occupied(_) => Err(PlinkoError::ItemExists(id)),
        Entry::Vacant(entry) => {
            entry.insert(item);
            Ok(())
        }
    }
}

fn update_map_item<T, P>(map: &mut HashMap<Uuid, T>, id: Uuid, patch: P) -> PlinkoResult<()>
where
    T: Patchable<P>,
{
    match map.entry(id) {
        Entry::Occupied(entry) => {
            entry.into_mut().apply_patch(patch)?;
            Ok(())
        }
        Entry::Vacant(_) => Err(PlinkoError::ItemDoesNotExist(id)),
    }
}

fn remove_item<T>(map: &mut HashMap<Uuid, T>, id: Uuid) -> PlinkoResult<()> {
    map.remove(&id)
        .ok_or(PlinkoError::ItemDoesNotExist(id))
        .map(drop)
} // }}}

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

#[derive(Debug, PartialEq)]
pub struct PlanVolatileState {
    pub tasks: HashMap<Uuid, task::VolatileState>,
    pub milestones: HashMap<Uuid, milestone::VolatileState>,
}

// impl PlanVolatileState{{{
impl PlanVolatileState {
    pub fn new(
        tasks: HashMap<Uuid, task::VolatileState>,
        milestones: HashMap<Uuid, milestone::VolatileState>,
    ) -> Self {
        Self { tasks, milestones }
    }
} // }}}
