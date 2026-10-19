// modules{{{
use std::collections::{HashMap, hash_map::Entry};

use uuid::Uuid;

use crate::{
    traits::{identified::Identified, patch::Patchable},
    types::{
        crud::Crud,
        error::{PlinkoError, PlinkoResult},
    },
};
// }}}

/// A collection of things stored in a container `C` (a plan or a tenant) and changed with `Crud`
pub trait Entity<C>: Sized + Clone + Patchable<Self::Patch> {
    type New: Identified + Into<Self>;
    type Patch: Identified;

    fn entities(container: &C) -> &HashMap<Uuid, Self>;
    fn entities_mut(container: &mut C) -> &mut HashMap<Uuid, Self>;

    /// Checked after every add or update, a failure reverts the change
    fn validate(_container: &C, _id: Uuid) -> PlinkoResult<()> {
        Ok(())
    }

    /// Whether something else still refers to the entity, which blocks removal
    fn in_use(_container: &C, _id: Uuid) -> bool {
        false
    }

    /// Applies the change then validates it, putting the entity back as it was on failure
    fn apply_crud(container: &mut C, update: Crud<Self::New, Self::Patch>) -> PlinkoResult<()> {
        match update {
            Crud::Add(new) => {
                let id = new.id();
                match Self::entities_mut(container).entry(id) {
                    Entry::Occupied(_) => return Err(PlinkoError::ItemExists(id)),
                    Entry::Vacant(entry) => entry.insert(new.into()),
                };
                Self::validate(container, id).inspect_err(|_| {
                    Self::entities_mut(container).remove(&id);
                })
            }
            Crud::Update(patch) => {
                let id = patch.id();
                let entity = Self::entities_mut(container)
                    .get_mut(&id)
                    .ok_or(PlinkoError::ItemDoesNotExist(id))?;
                let old = entity.clone();
                let result = entity
                    .apply_patch(patch)
                    .and_then(|()| Self::validate(container, id));
                if result.is_err() {
                    Self::entities_mut(container).insert(id, old);
                }
                result
            }
            Crud::Remove(id) => {
                if Self::in_use(container, id) {
                    return Err(PlinkoError::ItemInUse(id));
                }
                Self::entities_mut(container)
                    .remove(&id)
                    .map(drop)
                    .ok_or(PlinkoError::ItemDoesNotExist(id))
            }
        }
    }
}
