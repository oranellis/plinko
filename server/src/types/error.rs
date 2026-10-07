// modules{{{
use std::fmt::Debug;

use uuid::Uuid;

use crate::types::short_id::ShortId;
// }}}

#[derive(Debug)]
pub enum PlinkoError {
    Generic(String),
    InvalidWorkload,
    InvalidShortId(String),
    PlanExists(ShortId),
    PlanDoesNotExist(ShortId),
    GroupCycle(Uuid),
    DependencyCycle(Uuid),
    ItemInUse(Uuid),
    InvalidRule(Uuid),
    LastAdmin,
    Forbidden(Uuid),
    ItemExists(Uuid),
    ItemDoesNotExist(Uuid),
    IdMismatch(String, String),
}

// impl PlinkoError{{{
impl std::fmt::Display for PlinkoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlinkoError::Generic(s) => write!(f, "{}", s),
            PlinkoError::InvalidWorkload => write!(f, "invalid workload"),
            PlinkoError::PlanExists(short_id) => {
                write!(f, "plan {} already exists in tenant", short_id)
            }
            PlinkoError::PlanDoesNotExist(short_id) => {
                write!(f, "plan {} does not exist in tenant", short_id)
            }
            PlinkoError::GroupCycle(id) => {
                write!(f, "task group {} would contain itself", id)
            }
            PlinkoError::DependencyCycle(id) => {
                write!(f, "item {} would create a dependency cycle", id)
            }
            PlinkoError::Forbidden(id) => {
                write!(f, "user {} is not allowed to make this change", id)
            }
            PlinkoError::LastAdmin => write!(f, "the tenant must keep an admin"),
            PlinkoError::InvalidRule(id) => {
                write!(f, "availability rule {} is invalid", id)
            }
            PlinkoError::ItemInUse(id) => {
                write!(f, "item {} is still referenced", id)
            }
            PlinkoError::ItemDoesNotExist(id) => {
                write!(f, "item {} does not exist", id)
            }
            PlinkoError::InvalidShortId(s) => {
                write!(f, "cannot create ShortId, invalid entry: {}", s)
            }
            PlinkoError::ItemExists(id) => {
                write!(f, "item {} already exists", id)
            }
            PlinkoError::IdMismatch(id1, id2) => {
                write!(f, "{} does not match {}", id1, id2)
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
// }}}

pub type PlinkoResult<T> = Result<T, PlinkoError>;
