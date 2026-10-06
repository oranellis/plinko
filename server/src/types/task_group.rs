// modules {{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{dependency::Dependency, error::PlinkoResult, parent::Parent},
};
// }}}

/// A single zero duration node around the items whose `parent` is this group (tasks, milestones,
/// buffers and nested groups). The group starts when `dependencies` are met, members anchor to that
/// start with `DependencyNode::ParentStart`, and the group ends once every member has ended. The
/// end edges are derived by the scheduler.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskGroup {
    id: Uuid,
    name: String,
    dependencies: Vec<Dependency>,
    parent: Parent,
}

// impl TaskGroup {{{
impl TaskGroup {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn parent(&self) -> Parent {
        self.parent
    }
}

impl From<NewTaskGroup> for TaskGroup {
    fn from(new: NewTaskGroup) -> Self {
        Self {
            id: new.id,
            name: new.name,
            dependencies: Vec::new(),
            parent: new.parent,
        }
    }
}

impl Patchable<TaskGroupPatch> for TaskGroup {
    fn apply_patch(&mut self, patch: TaskGroupPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(d) = patch.dependencies {
            self.dependencies = d;
        }
        if let Some(p) = patch.parent {
            self.parent = p;
        }
        Ok(())
    }
}
// }}}

pub struct NewTaskGroup {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

pub struct TaskGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub dependencies: Option<Vec<Dependency>>,
    pub parent: Option<Parent>,
}

pub struct VolatileState {}
