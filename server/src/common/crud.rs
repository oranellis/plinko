// modules {{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::traits::identified::Identified;
// }}}

#[derive(Clone, Serialize, Deserialize)]
pub enum Crud<N, P> {
    Add(N),
    Update(P),
    Remove(Uuid),
}

// impl Crud {{{
impl<N: Identified, P> Crud<N, P> {
    /// Replaces whatever id a client sent with an add
    pub fn assign_id(&mut self) {
        if let Crud::Add(new) = self {
            new.set_id(Uuid::new_v4());
        }
    }
}
// }}}
