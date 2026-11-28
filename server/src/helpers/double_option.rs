//! Serde for patch fields that tell leaving (`None`), clearing (`Some(None)`) and setting apart.
//! Use as `#[serde(default, skip_serializing_if = "Option::is_none", with = "double_option")]`.

// modules {{{
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// }}}

// functions {{{
pub fn serialize<T: Serialize, S: Serializer>(
    value: &Option<Option<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(inner) => inner.serialize(serializer),
        None => serializer.serialize_none(),
    }
}

pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Patch {
        #[serde(default, skip_serializing_if = "Option::is_none", with = "super")]
        value: Option<Option<u8>>,
    }

    #[test]
    fn keeps_clears_and_sets_survive_a_round_trip() {
        for value in [None, Some(None), Some(Some(3))] {
            let patch = Patch { value };
            let bytes = rmp_serde::to_vec_named(&patch).unwrap();
            assert_eq!(rmp_serde::from_slice::<Patch>(&bytes).unwrap(), patch);
        }
    }
}
// }}}
