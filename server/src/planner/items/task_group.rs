// modules {{{
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{dates::DateRange, error::PlinkoResult},
    helpers::functions::check_id_match,
    planner::items::{dependency::Dependency, parent::Parent},
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
    dependencies: Vec<Dependency>,
    parent: Parent,
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

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn parent(&self) -> Parent {
        self.parent
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

    /// The range the group covers. `start` is the day its dependencies let members start, and
    /// `members` are the ranges of its scheduled members (a milestone as its one day). With none,
    /// `EMPTY_DAYS` days from `start`. Call it with no members only when the group has no members
    /// at all, a group whose members are all unscheduled is unscheduled itself.
    pub fn place(&self, start: NaiveDate, members: &[DateRange]) -> Option<DateRange> {
        let first = members.iter().map(DateRange::start).min();
        let last = members.iter().map(DateRange::end).max();
        match (first, last) {
            (Some(first), Some(last)) => DateRange::new(first, last),
            _ => DateRange::new(start, start + Duration::days(EMPTY_DAYS)),
        }
    }
}

impl From<NewTaskGroup> for TaskGroup {
    fn from(new: NewTaskGroup) -> Self {
        Self {
            id: new.id,
            name: new.name,
            dependencies: Vec::new(),
            parent: new.parent,
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
            self.dependencies = Dependency::merged(d);
        }
        if let Some(p) = patch.parent {
            self.parent = p;
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
    pub dependencies: Option<Vec<Dependency>>,
    pub parent: Option<Parent>,
}

/// Written by the scheduler for every group it could schedule, see `TaskGroup::place`
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolatileState {
    pub date_range: DateRange,
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn range(start: u32, end: u32) -> DateRange {
        DateRange::new(d(start), d(end)).unwrap()
    }

    fn group() -> TaskGroup {
        TaskGroup {
            id: Uuid::new_v4(),
            name: "Group".to_string(),
            dependencies: Vec::new(),
            parent: Parent::Plan,
            description: None,
            data: DataBlocks::default(),
        }
    }

    #[test]
    fn groups_run_from_their_first_member_to_the_end_of_their_last() {
        // dependencies allow the 1st, the first member only starts on the 3rd
        let members = [range(5, 9), range(3, 6), range(8, 10)];
        assert_eq!(group().place(d(1), &members), Some(range(3, 10)));
    }

    #[test]
    fn empty_groups_last_three_calendar_days() {
        assert_eq!(group().place(d(3), &[]), Some(range(3, 6)));
        // the weekend counts
        assert_eq!(group().place(d(5), &[]), Some(range(5, 8)));
    }

    #[test]
    fn the_empty_length_stops_applying_once_there_are_members() {
        assert_eq!(group().place(d(3), &[range(3, 4)]), Some(range(3, 4)));
    }
}
// }}}
