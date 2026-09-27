// modules {{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match, traits::patch::Patchable, types::error::PlinkoResult,
};
// }}}

pub struct TaskGroup {
    id: Uuid,
    name: String,
    contained_tasks: Vec<Uuid>,
}

// impl TaskGroup {{{
impl TaskGroup {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn contained_tasks(&self) -> &Vec<Uuid> {
        &self.contained_tasks
    }
}

impl Patchable<TaskGroupPatch> for TaskGroup {
    fn apply_patch(&mut self, patch: TaskGroupPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(c) = patch.contained_tasks {
            self.contained_tasks = c;
        }
        Ok(())
    }
}
// }}}

pub struct TaskGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub contained_tasks: Option<Vec<Uuid>>,
}

pub struct VolatileState {}
