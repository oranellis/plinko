// modules {{{
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        Days, common::DateRange, dependency::Dependency, error::PlinkoResult, parent::Parent,
        workload::Workload,
    },
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    id: Uuid,
    name: String,
    parent: Parent,
    mode: ScheduleMode,
    duration: u32,
    time_constraint: TimeConstraint,
    dependencies: Vec<Dependency>,
    allocations: HashMap<Worker, Days>, // Days
    description: Option<String>,
    links: Vec<String>,
    persistent_state: PersistentState,
}

// impl Task {{{
impl Task {
    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl From<NewTask> for Task {
    fn from(new: NewTask) -> Self {
        Self {
            id: new.id,
            name: new.name,
            parent: new.parent,
            mode: ScheduleMode::Strict,
            duration: 0,
            time_constraint: TimeConstraint::None,
            dependencies: Vec::new(),
            allocations: HashMap::new(),
            description: None,
            links: Vec::new(),
            persistent_state: PersistentState::new(),
        }
    }
}

impl Patchable<TaskPatch> for Task {
    fn apply_patch(&mut self, patch: TaskPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(p) = patch.parent {
            self.parent = p
        }
        if let Some(m) = patch.mode {
            self.mode = m
        }
        if let Some(d) = patch.duration {
            self.duration = d
        }
        if let Some(t) = patch.time_constraint {
            self.time_constraint = t
        }
        if let Some(d) = patch.dependencies {
            self.dependencies = d
        }
        if let Some(a) = patch.allocations {
            self.allocations = a
        }
        if let Some(d) = patch.description {
            self.description = d
        }
        if let Some(l) = patch.links {
            self.links = l
        }
        if let Some(p) = patch.persistent_state {
            self.persistent_state = p
        }
        Ok(())
    }
}
// }}}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PersistentState {
    status: Status,
    actual_start_date: Option<NaiveDate>,
    actual_end_date: Option<NaiveDate>,
}

// impl PersistentState {{{
impl PersistentState {
    pub fn new() -> Self {
        Self {
            status: Status::NotStarted,
            actual_start_date: None,
            actual_end_date: None,
        }
    }
}
// }}}

#[derive(Debug, Default, PartialEq)]
pub struct VolatileState {
    pub date_range: Option<DateRange>,
    pub assigned_allocations: HashMap<Uuid, BTreeMap<NaiveDate, Workload>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    NotStarted,
    InProgress,
    OnHold,
    Completed,
    Dropped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScheduleMode {
    #[default]
    Strict,
    Flexible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Worker {
    Tag(Uuid),
    User(Uuid),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TimeConstraint {
    #[default]
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}

pub struct NewTask {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

pub struct TaskPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub parent: Option<Parent>,
    pub mode: Option<ScheduleMode>,
    pub duration: Option<u32>,
    pub time_constraint: Option<TimeConstraint>,
    pub dependencies: Option<Vec<Dependency>>,
    pub allocations: Option<HashMap<Worker, Days>>,
    pub description: Option<Option<String>>,
    pub links: Option<Vec<String>>,
    pub persistent_state: Option<PersistentState>,
}
