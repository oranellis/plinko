// modules {{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::calendar::Calendar,
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        common::DateRange,
        dependency::Dependency,
        error::{PlinkoError, PlinkoResult},
        parent::Parent,
        workload::Workload,
    },
};
// }}}

/// A single node around the items whose `parent` is this group (tasks, milestones, buffers and
/// nested groups). Its dependencies decide the day members can start (`ParentStart`). The group
/// then covers the first member's start to the last member's end, so it begins with its first
/// task and ends on the last day a member runs. With no members it lasts `placeholder_duration`
/// working days instead, and that stops applying as soon as a member is added.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskGroup {
    id: Uuid,
    name: String,
    dependencies: Vec<Dependency>,
    parent: Parent,
    placeholder_duration: u32,
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

    pub fn placeholder_duration(&self) -> u32 {
        self.placeholder_duration
    }

    /// The range the group covers. `start` is the day its dependencies let members start, and
    /// `members` are the ranges of its scheduled members (a milestone as its one day). With none,
    /// the placeholder duration in working days of `calendar` from `start`. Call it with no members
    /// only when the group has no members at all, a group whose members are all unscheduled is
    /// unscheduled itself. `None` if the calendar is too short.
    pub fn place(
        &self,
        start: NaiveDate,
        members: &[DateRange],
        calendar: &Calendar<Workload>,
    ) -> Option<DateRange> {
        let first = members.iter().map(DateRange::start).min();
        let last = members.iter().map(DateRange::end).max();
        match (first, last) {
            (Some(first), Some(last)) => DateRange::new(first, last),
            _ => {
                let end = calendar.end_after_workdays(start, self.placeholder_duration)?;
                DateRange::new(start, end)
            }
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
            placeholder_duration: 1,
        }
    }
}

impl Patchable<TaskGroupPatch> for TaskGroup {
    fn apply_patch(&mut self, patch: TaskGroupPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;
        if patch.placeholder_duration == Some(0) {
            return Err(PlinkoError::InvalidDuration);
        }

        if let Some(n) = patch.name {
            self.name = n;
        }
        if let Some(d) = patch.dependencies {
            self.dependencies = d;
        }
        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(d) = patch.placeholder_duration {
            self.placeholder_duration = d;
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
    pub placeholder_duration: Option<u32>,
}

/// Written by the scheduler for every group it could schedule, see `TaskGroup::place`
#[derive(Clone, Debug, PartialEq)]
pub struct VolatileState {
    pub date_range: DateRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::rule_set::RuleSet;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn range(start: u32, end: u32) -> DateRange {
        DateRange::new(d(start), d(end)).unwrap()
    }

    fn group(placeholder_duration: u32) -> TaskGroup {
        TaskGroup {
            id: Uuid::now_v7(),
            name: "Group".to_string(),
            dependencies: Vec::new(),
            parent: Parent::PlanStart,
            placeholder_duration,
        }
    }

    /// Weekdays around January 2024, which starts on a Monday
    fn calendar() -> Calendar<Workload> {
        RuleSet::default().calendar(d(1), d(31))
    }

    #[test]
    fn groups_run_from_their_first_member_to_the_end_of_their_last() {
        // dependencies allow the 1st, the first member only starts on the 3rd
        let members = [range(5, 9), range(3, 6), range(8, 10)];
        let placed = group(5).place(d(1), &members, &calendar());
        assert_eq!(placed, Some(range(3, 10)));
    }

    #[test]
    fn empty_groups_last_their_placeholder_duration_in_working_days() {
        // Wednesday to the following Tuesday is 5 working days, ending the day after
        assert_eq!(group(5).place(d(3), &[], &calendar()), Some(range(3, 10)));
        assert_eq!(group(1).place(d(3), &[], &calendar()), Some(range(3, 4)));
        assert_eq!(group(100).place(d(3), &[], &calendar()), None);
    }

    #[test]
    fn the_placeholder_stops_applying_once_there_are_members() {
        let g = group(20);
        assert_eq!(
            g.place(d(3), &[range(3, 4)], &calendar()),
            Some(range(3, 4))
        );
    }
}
