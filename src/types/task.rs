use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::types::{
    Days, dependency::Dependency, tag::TagUuid, user::UserUuid, workload::Workload,
};

pub struct Task {
    id: TaskUuid,
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

pub type TaskUuid = Uuid;

#[derive(Default)]
pub struct PersistentState {
    status: Status,
    actual_start_date: Option<NaiveDate>,
    actual_end_date: Option<NaiveDate>,
}

#[derive(Default)]
pub struct VolatileState {
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    assigned_allocations: HashMap<UserUuid, BTreeMap<NaiveDate, Workload>>,
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
    Tag(TagUuid),
    User(UserUuid),
}

enum Constraint {
    None,
    Earliest(NaiveDate),
    Latest(NaiveDate),
    Fixed(NaiveDate),
}
