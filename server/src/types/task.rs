use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::types::{Days, dependency::Dependency, error::PlinkoResult, workload::Workload};

#[derive(Debug, PartialEq)]
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
    pub fn new() -> PlinkoResult<Task> {
        todo!("implement task new")
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct PersistentState {
    status: Status,
    actual_start_date: Option<NaiveDate>,
    actual_end_date: Option<NaiveDate>,
}

#[derive(Debug, PartialEq)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

#[derive(Debug, Default, PartialEq)]
pub struct VolatileState {
    date_range: Option<DateRange>,
    assigned_allocations: HashMap<Uuid, BTreeMap<NaiveDate, Workload>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    NotStarted,
    InProgress,
    OnHold,
    Completed,
    Dropped,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum ScheduleMode {
    #[default]
    Strict,
    Flexible,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Worker {
    Tag(Uuid),
    User(Uuid),
}

#[derive(Debug, Default, PartialEq)]
pub enum Constraint {
    #[default]
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}
