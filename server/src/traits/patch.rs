// modules{{{
use crate::common::error::PlinkoResult;
// }}}

pub trait Patchable<P> {
    fn apply_patch(&mut self, patch: P) -> PlinkoResult<()>;
}
