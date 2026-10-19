use crate::types::error::{PlinkoError, PlinkoResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Workload {
    value: u8,
}

impl Workload {
    pub fn new(value: u8) -> PlinkoResult<Self> {
        if value <= 8 {
            Ok(Self { value })
        } else {
            Err(PlinkoError::InvalidWorkload)
        }
    }

    pub fn get(&self) -> u8 {
        self.value
    }
}

impl From<Workload> for u8 {
    fn from(workload: Workload) -> Self {
        workload.value
    }
}

impl TryFrom<u8> for Workload {
    type Error = PlinkoError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_zero_to_eight() {
        for v in 0..=8 {
            assert_eq!(Workload::try_from(v).unwrap().get(), v);
        }
    }

    #[test]
    fn rejects_above_eight() {
        assert!(Workload::try_from(9).is_err());
        assert!(Workload::new(9).is_err());
    }
}
