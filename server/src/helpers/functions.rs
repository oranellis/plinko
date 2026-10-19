// modules{{{
use std::fmt::Display;

use crate::common::error::{PlinkoError, PlinkoResult};
// }}}

pub fn check_id_match<T>(id1: T, id2: T) -> PlinkoResult<()>
where
    T: PartialEq,
    T: Display,
{
    (id1 == id2)
        .then_some(())
        .ok_or(PlinkoError::IdMismatch(id1.to_string(), id2.to_string()))
}
