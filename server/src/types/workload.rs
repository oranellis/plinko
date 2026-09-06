use crate::types::error::{PlinkoError, PlinkoResult};

#[derive(Debug, Default, PartialEq)]
pub struct Workload {
    value: u8,
}

impl Workload {
    pub fn new(value: u8) -> PlinkoResult<Self> {
        if value >= 0 && value <= 8 {
            Ok(Self { value: value })
        } else {
            Err(PlinkoError::InvalidWorkload)
        }
    }

    pub fn new_unchecked(value: u8) -> Self {
        Self { value: value }
    }

    pub fn get(&self) -> u8 {
        self.value
    }
}

impl TryFrom<u8> for Workload {
    type Error = PlinkoError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 8 {
            Ok(Self { value: value })
        } else {
            Err(PlinkoError::InvalidWorkload)
        }
    }
}
