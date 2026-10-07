//! The binary format of updates, snapshots and schedules on the wire and in storage. MessagePack
//! with field names, so a field added with a default later still reads old data, and with map
//! keys of any type, which `HashMap<Worker, Workload>` needs.

use serde::{Serialize, de::DeserializeOwned};

use crate::types::error::{PlinkoError, PlinkoResult};

pub fn encode<T: Serialize>(value: &T) -> PlinkoResult<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| PlinkoError::Codec(e.to_string()))
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> PlinkoResult<T> {
    rmp_serde::from_slice(bytes).map_err(|e| PlinkoError::Codec(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;
    use crate::types::{
        crud::Crud,
        short_id::ShortId,
        task::{TaskPatch, Worker},
        workload::Workload,
    };

    fn patch() -> TaskPatch {
        TaskPatch {
            id: Uuid::now_v7(),
            name: None,
            parent: None,
            mode: None,
            duration: Some(3),
            time_constraint: None,
            dependencies: None,
            allocations: Some(HashMap::from([
                (Worker::Placeholder, Workload::new(4).unwrap()),
                (Worker::Tag(Uuid::now_v7()), Workload::new(8).unwrap()),
            ])),
            affinities: None,
            actual_workers: None,
            description: Some(None),
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
        assert_eq!(got.description, Some(None));
        assert_eq!(got.allocations, sent.allocations);
        assert_eq!(got.duration, Some(3));
        assert_eq!(got.name, None);
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
