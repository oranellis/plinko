use uuid::Uuid;

/// The standard way to change a collection of items: add from `N`, patch with `P`, remove by id
pub enum Crud<N, P> {
    Add(N),
    Update(P),
    Remove(Uuid),
}
