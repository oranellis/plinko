// modules {{{
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{dependency::Dependency, error::PlinkoResult},
};
// }}}

/// A single zero duration node around a set of tasks and nested groups. The group starts when
/// `dependencies` are met, members anchor to that start with `DependencyNode::ParentStart`, and
/// the group ends once every member has ended. The end edges are derived by the scheduler.
#[derive(Debug, PartialEq)]
pub struct TaskGroup {
    id: Uuid,
    name: String,
    dependencies: Vec<Dependency>,
    contents: Vec<GroupMember>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupMember {
    Task(Uuid),
    TaskGroup(Uuid),
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

    pub fn contents(&self) -> &Vec<GroupMember> {
        &self.contents
    }
}

impl From<NewTaskGroup> for TaskGroup {
    fn from(new: NewTaskGroup) -> Self {
        Self {
            id: new.id,
            name: new.name,
            dependencies: Vec::new(),
            contents: Vec::new(),
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
        if let Some(c) = patch.contents {
            self.contents = c;
        }
        Ok(())
    }
}
// }}}

pub struct NewTaskGroup {
    pub id: Uuid,
    pub name: String,
}

pub struct TaskGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub dependencies: Option<Vec<Dependency>>,
    pub contents: Option<Vec<GroupMember>>,
}

pub struct VolatileState {}
