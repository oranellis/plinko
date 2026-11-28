//! Wire and storage format: MessagePack with field names, so fields added later with a default
//! still read old data, and with non-string map keys, which `HashMap<Worker, Workload>` needs.

// modules {{{
use serde::{Serialize, de::DeserializeOwned};

use crate::common::error::{PlinkoError, PlinkoResult};
// }}}

// functions {{{
pub fn encode<T: Serialize>(value: &T) -> PlinkoResult<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| PlinkoError::Codec(e.to_string()))
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> PlinkoResult<T> {
    rmp_serde::from_slice(bytes).map_err(|e| PlinkoError::Codec(e.to_string()))
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;
    use crate::{
        availability::workload::Workload,
        common::{crud::Crud, short_id::ShortId},
        planner::items::task::{TaskPatch, Worker},
    };

    fn patch() -> TaskPatch {
        TaskPatch {
            id: Uuid::new_v4(),
            name: None,
            parent: None,
            duration: Some(3),
            time_constraint: None,
            dependencies: None,
            allocations: Some(HashMap::from([
                (Worker::Placeholder, Workload::new_const(4)),
                (Worker::Tag(Uuid::new_v4()), Workload::new_const(8)),
            ])),
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        }
    }

    #[test]
    fn patches_keep_the_difference_between_leaving_clearing_and_setting() {
        let sent: Crud<(), TaskPatch> = Crud::Update(patch());
        let got: Crud<(), TaskPatch> = decode(&encode(&sent).unwrap()).unwrap();
        let (Crud::Update(sent), Crud::Update(got)) = (sent, got) else {
            panic!("an update");
        };
        assert_eq!(got.allocations, sent.allocations);
        assert_eq!(got.duration, Some(3));
        assert_eq!(got.name, None);
    }

    #[test]
    fn clearing_a_field_is_not_the_same_as_leaving_it() {
        use crate::availability::rule::RulePatch;
        let patch = |stop_day| RulePatch::<Workload> {
            id: Uuid::new_v4(),
            start_day: None,
            stop_day,
            kind: None,
            name: None,
        };
        for stop_day in [None, Some(None), Some(Some(chrono::NaiveDate::MIN))] {
            let got: RulePatch<Workload> = decode(&encode(&patch(stop_day)).unwrap()).unwrap();
            assert_eq!(got.stop_day, stop_day);
        }
    }

    #[test]
    fn invalid_values_are_refused_on_the_way_in() {
        assert!(decode::<Workload>(&encode(&9u8).unwrap()).is_err());
        assert_eq!(decode::<Workload>(&encode(&8u8).unwrap()).unwrap().get(), 8);
        assert!(decode::<ShortId>(&encode(&"short").unwrap()).is_err());
        let id = ShortId::new();
        assert_eq!(decode::<ShortId>(&encode(&id).unwrap()).unwrap(), id);
        let day = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        assert_eq!(decode::<NaiveDate>(&encode(&day).unwrap()).unwrap(), day);
        assert!(decode::<Workload>(&[0xc1]).is_err());
    }
}
// }}}
