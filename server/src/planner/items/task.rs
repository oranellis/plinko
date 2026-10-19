// modules {{{
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use crate::{
    availability::workload::Workload,
    common::{
        dates::{DateRange, TimeConstraint},
        error::{PlinkoError, PlinkoResult},
    },
    helpers::functions::check_id_match,
    planner::items::{dependency::Dependency, parent::Parent},
    planner::resources::data_block::DataBlocks,
    traits::patch::Patchable,
};
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Who actually does the work for a requested worker, which can be a different person (or the
    /// placeholder). It starts empty, is filled in as the task starts (by default with the people
    /// last scheduled for it) and is edited by updates. Slots without an entry use the scheduler's
    /// choice for tags and groups and the named person for users.
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

    /// Days, at least 1
    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn mode(&self) -> ScheduleMode {
        self.mode
    }

    pub fn time_constraint(&self) -> TimeConstraint {
        self.time_constraint
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
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
            mode: ScheduleMode::Strict,
            duration: 1,
            time_constraint: TimeConstraint::None,
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
            self.dependencies = Dependency::merged(d)
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
/// What users record as a task goes: its status and the days it really started and ended. A task
/// put on hold ends on the day that happens (see `Plan::apply_update`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistentState {
    pub status: Status,
    pub actual_start_date: Option<NaiveDate>,
    pub actual_end_date: Option<NaiveDate>,
}

/// Written by the scheduler for every task it could schedule, the others are unscheduled
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
    /// Hours per day each person works on the task, including days before today for work already
    /// done and for in progress tasks that overran their duration
    pub assignments: HashMap<Assignee, BTreeMap<NaiveDate, Workload>>,
    /// The person resolved for each worker the task asks for
    pub workers: HashMap<Worker, Assignee>,
}

/// Who actually does the work, as opposed to the `Worker` a task asks for, which can be a tag or
/// a user group that the scheduler resolves to people
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

/// How a task fills the days it is worked on
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleMode {
    /// Only days where the whole daily block of hours is free
    #[default]
    Strict,
    /// Any day with free time, using whatever is free up to the daily block
    Relaxed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Worker {
    Tag(Uuid),
    User(Uuid),
    UserGroup(Uuid),
    /// An unnamed stand in for whoever will do the work, always free and available per the plan
    /// default availability
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
    pub mode: Option<ScheduleMode>,
    pub duration: Option<u32>,
    pub time_constraint: Option<TimeConstraint>,
    pub dependencies: Option<Vec<Dependency>>,
    pub allocations: Option<HashMap<Worker, Workload>>,
    pub affinities: Option<HashSet<Uuid>>,
    pub actual_workers: Option<HashMap<Worker, Assignee>>,
    pub persistent_state: Option<PersistentState>,
}
// }}}

// impl ScheduleMode {{{
impl ScheduleMode {
    /// The hours worked on a day when `wanted` are asked for and `available` are free
    pub fn hours_on(self, wanted: Workload, available: Workload) -> Workload {
        match self {
            ScheduleMode::Strict if available.get() >= wanted.get() => wanted,
            ScheduleMode::Strict => Workload::default(),
            ScheduleMode::Relaxed => {
                Workload::new(wanted.get().min(available.get())).expect("at most the wanted hours")
            }
        }
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn hours(h: u8) -> Workload {
        Workload::new(h).unwrap()
    }

    #[test]
    fn strict_needs_the_whole_block_and_relaxed_takes_what_is_free() {
        let (strict, relaxed) = (ScheduleMode::Strict, ScheduleMode::Relaxed);
        assert_eq!(strict.hours_on(hours(4), hours(8)), hours(4));
        assert_eq!(strict.hours_on(hours(4), hours(4)), hours(4));
        assert_eq!(strict.hours_on(hours(4), hours(3)), hours(0));
        assert_eq!(relaxed.hours_on(hours(4), hours(8)), hours(4));
        assert_eq!(relaxed.hours_on(hours(4), hours(3)), hours(3));
        assert_eq!(relaxed.hours_on(hours(4), hours(0)), hours(0));
    }
}
// }}}
