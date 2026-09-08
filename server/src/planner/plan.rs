use std::collections::{HashMap, hash_map::Entry};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    planner::plan_update::{
        MilestoneUpdate, PlanUpdate, PlanUpdateType, SchedulerDataUpdate, TagUpdate, TaskUpdate,
        UserUpdate,
    },
    types::{
        dependency::PlanNode,
        error::{PlinkoError, PlinkoResult},
        milestone::{self, Milestone},
        schedule::{CalendarChanges, WeeklySchedule, WeeklySchedules},
        short_id::ShortId,
        tag::Tag,
        task::{self, Task},
        user::User,
    },
};

#[derive(Debug, PartialEq)]
pub struct Plan {
    id: ShortId,
    name: String,
    tasks: HashMap<Uuid, Task>,
    milestones: HashMap<Uuid, Milestone>,
    users: HashMap<Uuid, User>,
    tags: HashMap<Uuid, Tag>,
    plan_weekly_schedule: WeeklySchedules,
    plan_calendar_changes: CalendarChanges,
    scheduler_data: PlanSchedulerData,
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

    pub fn milestones(&self) -> &HashMap<Uuid, Milestone> {
        &self.milestones
    }

    pub fn users(&self) -> &HashMap<Uuid, User> {
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

    pub fn new(id: ShortId, name: String, start_date: NaiveDate) -> Self {
        Self {
            id,
            name,
            tasks: HashMap::new(),
            milestones: HashMap::new(),
            users: HashMap::new(),
            tags: HashMap::new(),
            plan_weekly_schedule: WeeklySchedules::new(WeeklySchedule::new_initial_schedule()),
            plan_calendar_changes: CalendarChanges::new(),
            scheduler_data: PlanSchedulerData::new(start_date),
        }
    }

    // pub fn ensure_uuid_exists(&self, id: &Uuid) -> PlinkoResult<()> {
    //     (self.tags.contains_key(id)
    //         || self.users.contains_key(id)
    //         || self.milestones.contains_key(id)
    //         || self.tasks.contains_key(id))
    //     .then_some(())
    //     .ok_or(PlinkoError::ItemDoesNotExist(*id))
    // }

    pub fn ensure_node_exists(&self, node: &PlanNode) -> PlinkoResult<()> {
        match node {
            PlanNode::PlanStart => Ok(()),
            PlanNode::Task(id) => self
                .tasks
                .contains_key(id)
                .then_some(())
                .ok_or(PlinkoError::ItemDoesNotExist(*id)),
            PlanNode::Milestone(id) => self
                .milestones
                .contains_key(id)
                .then_some(())
                .ok_or(PlinkoError::ItemDoesNotExist(*id)),
        }
    }

    pub fn apply_update(&mut self, u: PlanUpdate) -> PlinkoResult<()> {
        match u.update_type {
            PlanUpdateType::Task(update) => self.task_update(update),
            PlanUpdateType::User(update) => self.user_update(update),
            PlanUpdateType::Milestone(update) => self.milestone_update(update),
            PlanUpdateType::Tag(update) => self.tag_update(update),
            PlanUpdateType::SchedulerData(update) => self.scheduler_data_update(update),
        }
    }

    fn task_update(&mut self, update: TaskUpdate) -> PlinkoResult<()> {
        match update {
            TaskUpdate::Add(task) => add_item(&mut self.tasks, *task.id(), task),
            TaskUpdate::Update(task) => update_item(&mut self.tasks, *task.id(), task),
            TaskUpdate::Remove(id) => remove_item(&mut self.tasks, id),
        }
    }

    fn user_update(&mut self, update: UserUpdate) -> PlinkoResult<()> {
        match update {
            UserUpdate::Add(user) => add_item(&mut self.users, *user.id(), user),
            UserUpdate::Update(user) => update_item(&mut self.users, *user.id(), user),
            UserUpdate::Remove(id) => remove_item(&mut self.users, id),
        }
    }

    fn milestone_update(&mut self, update: MilestoneUpdate) -> PlinkoResult<()> {
        match update {
            MilestoneUpdate::Add(m) => add_item(&mut self.milestones, *m.id(), m),
            MilestoneUpdate::Update(m) => update_item(&mut self.milestones, *m.id(), m),
            MilestoneUpdate::Remove(id) => remove_item(&mut self.milestones, id),
        }
    }

    fn tag_update(&mut self, update: TagUpdate) -> PlinkoResult<()> {
        match update {
            TagUpdate::Add(tag) => add_item(&mut self.tags, *tag.id(), tag),
            TagUpdate::Update(tag) => update_item(&mut self.tags, *tag.id(), tag),
            TagUpdate::Remove(id) => remove_item(&mut self.tags, id),
        }
    }

    fn scheduler_data_update(&mut self, update: SchedulerDataUpdate) -> Result<(), PlinkoError> {
        match update {
            SchedulerDataUpdate::Update(plan_scheduler_data) => {
                self.ensure_node_exists(&plan_scheduler_data.scheduler_target)?;
                self.scheduler_data = plan_scheduler_data;
                Ok(())
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

fn update_item<T>(map: &mut HashMap<Uuid, T>, id: Uuid, item: T) -> PlinkoResult<()> {
    match map.entry(id) {
        Entry::Occupied(mut entry) => {
            entry.insert(item);
            Ok(())
        }
        Entry::Vacant(_) => Err(PlinkoError::ItemDoesNotExist(id)),
    }
}

fn remove_item<T>(map: &mut HashMap<Uuid, T>, id: Uuid) -> PlinkoResult<()> {
    map.remove(&id)
        .ok_or(PlinkoError::ItemDoesNotExist(id))
        .map(drop)
}

#[derive(Debug, PartialEq)]
pub struct PlanSchedulerData {
    start_date: NaiveDate,
    scheduler_target: PlanNode,
}

impl PlanSchedulerData {
    fn new(start_date: NaiveDate) -> Self {
        Self {
            start_date,
            scheduler_target: PlanNode::PlanStart,
        }
    }

    pub fn set_start_date(&mut self, date: NaiveDate) {
        self.start_date = date;
    }

    pub fn set_scheduler_target(&mut self, node: PlanNode) {
        self.scheduler_target = node;
    }

    pub fn start_date(&self) -> &NaiveDate {
        &self.start_date
    }

    pub fn scheduler_target(&self) -> &PlanNode {
        &self.scheduler_target
    }
}

#[derive(Debug, PartialEq)]
pub struct PlanVolatileState {
    pub tasks: HashMap<Uuid, task::VolatileState>,
    pub milestones: HashMap<Uuid, milestone::VolatileState>,
}

impl PlanVolatileState {
    pub fn new(
        tasks: HashMap<Uuid, task::VolatileState>,
        milestones: HashMap<Uuid, milestone::VolatileState>,
    ) -> Self {
        Self { tasks, milestones }
    }
}
