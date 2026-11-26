// modules {{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{
        dates::{DateRange, NodeConstraint},
        error::PlinkoResult,
    },
    helpers::functions::check_id_match,
    planner::items::{dependency::PlanNode, parent::Parent},
    planner::resources::data_block::DataBlocks,
    traits::patch::Patchable,
};
// }}}

// constants {{{
/// A single node around the items whose `parent` is this group (tasks, milestones, buffers and
/// nested groups). Its dependencies decide the day members can start (`ParentStart`). The group
/// then covers the first member's start to the last member's end, so it begins with its first
/// task and ends on the last day a member runs. With no members it lasts `EMPTY_DAYS` calendar
/// days instead, and that stops applying as soon as a member is added.
/// How many calendar days a group with no members lasts
pub const EMPTY_DAYS: i64 = 3;
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskGroup {
    id: Uuid,
    name: String,
    dependencies: Vec<PlanNode>,
    parent: Parent,
    /// A date for the first day of its first member or the last day of its last
    time_constraint: NodeConstraint,
    description: Option<String>,
    data: DataBlocks,
}

// impl TaskGroup {{{
impl TaskGroup {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dependencies(&self) -> &Vec<PlanNode> {
        &self.dependencies
    }

    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn time_constraint(&self) -> NodeConstraint {
        self.time_constraint
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub(crate) fn set_description(&mut self, description: Option<String>) {
        self.description = description;
    }

    pub fn data(&self) -> &DataBlocks {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut DataBlocks {
        &mut self.data
    }
}

impl From<NewTaskGroup> for TaskGroup {
    fn from(new: NewTaskGroup) -> Self {
        Self {
            id: new.id,
            name: new.name,
            dependencies: Vec::new(),
            parent: new.parent,
            time_constraint: NodeConstraint::default(),
            description: None,
            data: DataBlocks::default(),
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
            self.dependencies = PlanNode::merged(d);
        }
        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(t) = patch.time_constraint {
            self.time_constraint = t;
        }
        Ok(())
    }
}
// }}}

// TaskGroup supporting types {{{
#[derive(Clone, Serialize, Deserialize)]
pub struct NewTaskGroup {
    pub id: Uuid,
    pub name: String,
    pub parent: Parent,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TaskGroupPatch {
    pub id: Uuid,
    pub name: Option<String>,
    pub dependencies: Option<Vec<PlanNode>>,
    pub parent: Option<Parent>,
    pub time_constraint: Option<NodeConstraint>,
}

/// Written by the scheduler for every group it could schedule
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
}
// }}}
