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

#[derive(Debug, PartialEq)]
pub struct VolatileState {
    derived_status: Status,
    date: NaiveDate,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Status {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}
