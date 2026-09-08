use chrono::NaiveDate;
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Milestone {
    id: Uuid,
    name: String,
    persistent_state: PersistentState,
}

impl Milestone {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            name,
            persistent_state: PersistentState::new(),
        }
    }
    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

#[derive(Debug, PartialEq)]
pub struct PersistentState {
    actual_date: Option<NaiveDate>,
}

impl PersistentState {
    pub fn new() -> Self {
        Self { actual_date: None }
    }
}

#[derive(Debug, PartialEq)]
pub struct VolatileState {
    derived_status: Status,
    date: NaiveDate,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}
