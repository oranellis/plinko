use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    Fixed,
    Earliest,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateConstraint {
    pub date: NaiveDate,
    pub kind: ConstraintKind,
}

impl DateConstraint {
    pub fn fixed(date: NaiveDate) -> Self {
        Self {
            date,
            kind: ConstraintKind::Fixed,
        }
    }

    pub fn earliest(date: NaiveDate) -> Self {
        Self {
            date,
            kind: ConstraintKind::Earliest,
        }
    }

    pub fn latest(date: NaiveDate) -> Self {
        Self {
            date,
            kind: ConstraintKind::Latest,
        }
    }
}
