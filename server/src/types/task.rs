use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::types::{Days, dependency::Dependency, error::PlinkoResult, workload::Workload};

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    id: Uuid,
    name: String,
    mode: ScheduleMode,
    duration: u32,
    constraint: Constraint,
    dependencies: Vec<Dependency>,
    allocations: HashMap<Worker, Days>, // Days
    description: Option<String>,
    links: Vec<String>,
    persistent_state: PersistentState,
}

impl Task {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            mode: ScheduleMode::Strict,
            duration: 0,
            constraint: Constraint::None,
            dependencies: Vec::new(),
            allocations: HashMap::new(),
            description: None,
            links: Vec::new(),
            persistent_state: PersistentState::new(),
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PersistentState {
    status: Status,
    actual_start_date: Option<NaiveDate>,
    actual_end_date: Option<NaiveDate>,
}

impl PersistentState {
    pub fn new() -> Self {
        Self {
            status: Status::NotStarted,
            actual_start_date: None,
            actual_end_date: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

#[derive(Debug, Default, PartialEq)]
pub struct VolatileState {
    date_range: Option<DateRange>,
    assigned_allocations: HashMap<Uuid, BTreeMap<NaiveDate, Workload>>,
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
pub enum Constraint {
    #[default]
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}
