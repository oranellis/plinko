use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::types::{Days, dependency::Dependency, workload::Workload};

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

#[derive(Default)]
pub struct PersistentState {
    status: Status,
    actual_start_date: Option<NaiveDate>,
    actual_end_date: Option<NaiveDate>,
}

pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

#[derive(Default)]
pub struct VolatileState {
    date_range: Option<DateRange>,
    assigned_allocations: HashMap<Uuid, BTreeMap<NaiveDate, Workload>>,
}

#[derive(Default)]
enum Status {
    #[default]
    NotStarted,
    InProgress,
    OnHold,
    Completed,
    Dropped,
}

#[derive(Default)]
enum ScheduleMode {
    #[default]
    Strict,
    Flexible,
}

enum Worker {
    Tag(Uuid),
    User(Uuid),
}

enum Constraint {
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}
