use chrono::NaiveDate;
use uuid::Uuid;

pub struct Milestone {
    id: Uuid,
    name: String,
    persistent_state: PersistentState,
}

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
