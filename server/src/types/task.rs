// modules {{{
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        common::{DateRange, TimeConstraint},
        data_block::DataBlocks,
        dependency::Dependency,
        error::PlinkoResult,
        parent::Parent,
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
    /// Hours per day each worker spends on the task for its duration
    allocations: HashMap<Worker, Workload>,
    /// Linked affinities, the task is only worked on days that every one of them allows
    affinities: HashSet<Uuid>,
    description: Option<String>,
    data: DataBlocks,
    persistent_state: PersistentState,
}

// impl Task {{{
impl Task {
    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn allocations(&self) -> &HashMap<Worker, Workload> {
        &self.allocations
    }

    pub fn affinities(&self) -> &HashSet<Uuid> {
        &self.affinities
    }

    pub fn data(&self) -> &DataBlocks {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut DataBlocks {
        &mut self.data
    }

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
            affinities: HashSet::new(),
            description: None,
            data: DataBlocks::default(),
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
        if let Some(a) = patch.affinities {
            self.affinities = a
        }
        if let Some(d) = patch.description {
            self.description = d
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

/// Written by the scheduler for every task it could schedule, the others are unscheduled
#[derive(Clone, Debug, PartialEq)]
pub struct VolatileState {
    pub date_range: DateRange,
    /// Hours per day each person works on the task, including days before today for work already
    /// done and for in progress tasks that overran their duration
    pub assignments: HashMap<Assignee, BTreeMap<NaiveDate, Workload>>,
}

/// Who actually does the work, as opposed to the `Worker` a task asks for, which can be a tag or
/// a user group that the scheduler resolves to people
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Assignee {
    User(Uuid),
    /// Unlimited, never conflicts with other work
    Placeholder,
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
    UserGroup(Uuid),
    /// An unnamed stand in for whoever will do the work, always free and available per the plan
    /// default availability
    Placeholder,
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
    pub allocations: Option<HashMap<Worker, Workload>>,
    pub affinities: Option<HashSet<Uuid>>,
    pub description: Option<Option<String>>,
    pub persistent_state: Option<PersistentState>,
}
