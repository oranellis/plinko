use chrono::NaiveDate;
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Milestone {
    id: Uuid,
    name: String,
    persistent_state: PersistentState,
}

#[derive(Debug, PartialEq)]
pub struct PersistentState {
    actual_date: Option<NaiveDate>,
}

pub struct VolatileState {
    derived_status: Status,
    date: NaiveDate,
}

#[derive(Default)]
enum Status {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}
