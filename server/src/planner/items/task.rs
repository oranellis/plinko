// modules {{{
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use crate::{
    availability::workload::Workload,
    common::{
        dates::{DateRange, NodeConstraint},
        error::{PlinkoError, PlinkoResult},
    },
    helpers::functions::check_id_match,
    planner::items::{dependency::PlanNode, parent::Parent},
    planner::resources::data_block::DataBlocks,
    traits::patch::Patchable,
};
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    id: Uuid,
    name: String,
    parent: Parent,
    /// Working days, in one unbroken run
    duration: u32,
    time_constraint: NodeConstraint,
    dependencies: Vec<PlanNode>,
    /// Hours per day
    allocations: HashMap<Worker, Workload>,
    /// Only worked on days every one of them allows
    affinities: HashSet<Uuid>,
    /// Filled in when the task starts, by default with whoever was last scheduled. Slots without
    /// an entry fall back to the scheduler's choice, or the named user.
    actual_workers: HashMap<Worker, Assignee>,
    description: Option<String>,
    data: DataBlocks,
    persistent_state: PersistentState,
}

// impl Task {{{
impl Task {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub(crate) fn set_description(&mut self, description: Option<String>) {
        self.description = description;
    }

    /// At least 1
    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn time_constraint(&self) -> NodeConstraint {
        self.time_constraint
    }

    pub fn dependencies(&self) -> &Vec<PlanNode> {
        &self.dependencies
    }

    pub fn allocations(&self) -> &HashMap<Worker, Workload> {
        &self.allocations
    }

    pub fn affinities(&self) -> &HashSet<Uuid> {
        &self.affinities
    }

    pub fn actual_workers(&self) -> &HashMap<Worker, Assignee> {
        &self.actual_workers
    }

    pub fn data(&self) -> &DataBlocks {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut DataBlocks {
        &mut self.data
    }

    pub fn persistent_state(&self) -> PersistentState {
        self.persistent_state
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
            duration: 1,
            time_constraint: NodeConstraint::default(),
            dependencies: Vec::new(),
            allocations: HashMap::new(),
            affinities: HashSet::new(),
            actual_workers: HashMap::new(),
            description: None,
            data: DataBlocks::default(),
            persistent_state: PersistentState::default(),
        }
    }
}

impl Patchable<TaskPatch> for Task {
    fn apply_patch(&mut self, patch: TaskPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;
        if patch.duration == Some(0) {
            return Err(PlinkoError::InvalidDuration);
        }

        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(p) = patch.parent {
            self.parent = p
        }
        if let Some(d) = patch.duration {
            self.duration = d
        }
        if let Some(t) = patch.time_constraint {
            self.time_constraint = t
        }
        if let Some(d) = patch.dependencies {
            self.dependencies = PlanNode::merged(d)
        }
        if let Some(a) = patch.allocations {
            self.allocations = a
        }
        if let Some(a) = patch.affinities {
            self.affinities = a
        }
        if let Some(w) = patch.actual_workers {
            self.actual_workers = w
        }
        if let Some(p) = patch.persistent_state {
            self.persistent_state = p
        }
        Ok(())
    }
}
// }}}

// Task supporting types {{{
/// Recorded by users. A task put on hold ends that day (see `Plan::apply_update`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistentState {
    pub status: Status,
    pub actual_start_date: Option<NaiveDate>,
    pub actual_end_date: Option<NaiveDate>,
}

/// Absent when the scheduler could not place the task
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
    /// Includes past days of work already done, and overruns of in-progress tasks
    pub assignments: HashMap<Assignee, BTreeMap<NaiveDate, Workload>>,
    pub workers: HashMap<Worker, Assignee>,
}

/// A `Worker` resolved to a person
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Assignee {
    User(Uuid),
    /// Unlimited, never conflicts with other work
    Placeholder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    #[default]
    NotStarted,
    InProgress,
    OnHold,
    Completed,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Worker {
    Tag(Uuid),
    User(Uuid),
    UserGroup(Uuid),
    /// Always free, available per the plan's default availability
    Placeholder,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewTask {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TaskPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub parent: Option<Parent>,
    pub duration: Option<u32>,
    pub time_constraint: Option<NodeConstraint>,
    pub dependencies: Option<Vec<PlanNode>>,
    pub allocations: Option<HashMap<Worker, Workload>>,
    pub affinities: Option<HashSet<Uuid>>,
    pub actual_workers: Option<HashMap<Worker, Assignee>>,
    pub persistent_state: Option<PersistentState>,
}
// }}}
