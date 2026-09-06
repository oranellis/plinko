use std::fmt::Debug;

use crate::types::short_id::ShortId;

#[derive(Debug)]
pub enum PlinkoError {
    Generic(String),
    InvalidWorkload,
    InvalidShortId(String),
    PlanExists(ShortId),
    EntryDoesNotExist(Box<dyn Debug>),
}

impl std::fmt::Display for PlinkoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlinkoError::Generic(s) => write!(f, "{}", s),
            PlinkoError::InvalidWorkload => write!(f, "invalid workload"),
            PlinkoError::PlanExists(short_id) => {
                write!(f, "plan {} already exists in tenant", short_id)
            }
            PlinkoError::EntryDoesNotExist(entry) => {
                write!(f, "entry {:?} does not exist", entry)
            }
            PlinkoError::InvalidShortId(s) => {
                write!(f, "cannot create ShortId, invalid entry: {}", s)
            }
        }
    }
}

impl std::error::Error for PlinkoError {}

impl From<String> for PlinkoError {
    fn from(value: String) -> Self {
        PlinkoError::Generic(value)
    }
}

impl From<&str> for PlinkoError {
    fn from(value: &str) -> Self {
        PlinkoError::Generic(value.to_string())
    }
}

pub type PlinkoResult<T> = Result<T, PlinkoError>;
