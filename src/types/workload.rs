use crate::types::error::{PlinkoError, PlinkoResult};

#[derive(Default)]
pub struct Workload {
    value: u8,
}

impl Workload {
    fn new(value: u8) -> PlinkoResult<Self> {
        if value > 8 {
            Ok(Self { value: value })
        } else {
            Err(PlinkoError::InvalidWorkload)
        }
    }

    fn value(&self) -> u8 {
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
