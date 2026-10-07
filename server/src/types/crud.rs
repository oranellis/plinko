use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::traits::identified::Identified;

#[derive(Serialize, Deserialize)]
/// The standard way to change a collection of items: add from `N`, patch with `P`, remove by id
pub enum Crud<N, P> {
    Add(N),
    Update(P),
    Remove(Uuid),
}

impl<N: Identified, P> Crud<N, P> {
    /// Gives an add a fresh id, replacing whatever a client sent. Updates and removals name
    /// existing items, so they are left alone.
    pub fn assign_id(&mut self) {
        if let Crud::Add(new) = self {
            new.set_id(Uuid::now_v7());
        }
    }
}
