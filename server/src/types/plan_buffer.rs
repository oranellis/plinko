// modules {{{
use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::calendar::Calendar,
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{
        common::DateRange, dependency::Dependency, error::PlinkoResult, parent::Parent,
        workload::Workload,
    },
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanBuffer {
    id: Uuid,
    parent: Parent,
    dependencies: Vec<Dependency>,
    set_start_day: Option<NaiveDate>,
    duration: u32,
}

// impl PlanBuffer {{{
impl PlanBuffer {
    pub fn parent(&self) -> Parent {
        self.parent
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn dependencies(&self) -> &Vec<Dependency> {
        &self.dependencies
    }

    pub fn set_start_day(&self) -> &Option<NaiveDate> {
        &self.set_start_day
    }

    pub fn duration(&self) -> &u32 {
        &self.duration
    }

    /// Where the buffer sits when its dependencies finish on `start`, with `duration` counted in
    /// working days of `calendar` (normally the plan's default availability).
    ///
    /// The buffer always starts on `start`. Unset, it runs for its duration from there. Set, it
    /// ends where it would if it had started on `set_start_day`, so it shrinks when `start` is
    /// later and grows when it is earlier, down to nothing once `start` passes that end. `None`
    /// if the calendar does not reach far enough, it must cover `set_start_day` when that is set.
    ///
    /// `limit` is the latest the buffer may end, set by dependents that cannot move later. It
    /// shrinks the buffer, set or not, but never below `start`.
    pub fn place(
        &self,
        start: NaiveDate,
        calendar: &Calendar<Workload>,
        limit: Option<NaiveDate>,
    ) -> Option<DateRange> {
        let end = match self.set_start_day {
            None => calendar.end_after_workdays(start, self.duration)?,
            Some(set) => calendar.end_after_workdays(set, self.duration)?.max(start),
        };
        DateRange::new(start, end.min(limit.unwrap_or(end)).max(start))
    }
}

impl From<NewPlanBuffer> for PlanBuffer {
    fn from(new: NewPlanBuffer) -> Self {
        Self {
            id: new.id,
            parent: new.parent,
            dependencies: Vec::new(),
            set_start_day: None,
            duration: new.duration,
        }
    }
}

impl Patchable<PlanBufferPatch> for PlanBuffer {
    fn apply_patch(&mut self, patch: PlanBufferPatch) -> PlinkoResult<()> {
        check_id_match(*self.id(), patch.id)?;

        if let Some(p) = patch.parent {
            self.parent = p;
        }
        if let Some(p) = patch.dependencies {
            self.dependencies = p;
        }
        if let Some(p) = patch.set_start_day {
            self.set_start_day = p
        }
        if let Some(p) = patch.duration {
            self.duration = p;
        }
        Ok(())
    }
}
// }}}

/// Written by the scheduler for every buffer it could schedule, see `PlanBuffer::place`
#[derive(Clone, Debug, PartialEq)]
pub struct VolatileState {
    pub date_range: DateRange,
}

pub struct NewPlanBuffer {
    pub id: Uuid,
    pub parent: Parent,
    pub duration: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanBufferPatch {
    pub id: Uuid,
    pub parent: Option<Parent>,
    pub dependencies: Option<Vec<Dependency>>,
    pub set_start_day: Option<Option<NaiveDate>>,
    pub duration: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::rule_set::RuleSet;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn buffer(set_start_day: Option<NaiveDate>, duration: u32) -> PlanBuffer {
        PlanBuffer {
            id: Uuid::now_v7(),
            parent: Parent::PlanStart,
            dependencies: Vec::new(),
            set_start_day,
            duration,
        }
    }

    /// Weekday availability for a few weeks around January 2024, which starts on a Monday
    fn calendar() -> Calendar<Workload> {
        RuleSet::default().calendar(d(2023, 12, 18), d(2024, 2, 28))
    }

    fn placed(buffer: &PlanBuffer, start: NaiveDate) -> (NaiveDate, NaiveDate) {
        let range = buffer.place(start, &calendar(), None).unwrap();
        (range.start(), range.end())
    }

    #[test]
    fn unset_buffers_keep_their_duration_in_working_days() {
        let b = buffer(None, 5);
        // Monday to Friday, ending the day after the fifth working day
        assert_eq!(placed(&b, d(2024, 1, 1)), (d(2024, 1, 1), d(2024, 1, 6)));
        // Wednesday on: Wed, Thu, Fri, Mon, Tue
        assert_eq!(placed(&b, d(2024, 1, 3)), (d(2024, 1, 3), d(2024, 1, 10)));
        // starting on a Saturday counts from the Monday
        assert_eq!(placed(&b, d(2024, 1, 6)), (d(2024, 1, 6), d(2024, 1, 13)));
    }

    #[test]
    fn set_buffers_shrink_and_grow_around_a_fixed_end() {
        let b = buffer(Some(d(2024, 1, 1)), 5);
        let end = d(2024, 1, 6);
        assert_eq!(placed(&b, d(2024, 1, 1)), (d(2024, 1, 1), end));
        // dependencies finish later, so the buffer shrinks
        assert_eq!(placed(&b, d(2024, 1, 3)), (d(2024, 1, 3), end));
        // dependencies finish earlier, so it grows
        assert_eq!(placed(&b, d(2023, 12, 25)), (d(2023, 12, 25), end));
    }

    #[test]
    fn set_buffers_shrink_to_nothing_but_never_below() {
        let b = buffer(Some(d(2024, 1, 1)), 5);
        assert_eq!(placed(&b, d(2024, 1, 6)), (d(2024, 1, 6), d(2024, 1, 6)));
        assert_eq!(placed(&b, d(2024, 1, 15)), (d(2024, 1, 15), d(2024, 1, 15)));
    }

    #[test]
    fn limits_shrink_set_and_unset_buffers_but_not_below_their_start() {
        let limit = Some(d(2024, 1, 4));
        let cut = |b: &PlanBuffer, start| {
            let range = b.place(start, &calendar(), limit).unwrap();
            (range.start(), range.end())
        };
        assert_eq!(
            cut(&buffer(None, 5), d(2024, 1, 1)),
            (d(2024, 1, 1), d(2024, 1, 4))
        );
        let set = buffer(Some(d(2024, 1, 1)), 5);
        assert_eq!(cut(&set, d(2024, 1, 2)), (d(2024, 1, 2), d(2024, 1, 4)));
        // already past the limit, so it is empty and the dependent is late
        assert_eq!(cut(&set, d(2024, 1, 8)), (d(2024, 1, 8), d(2024, 1, 8)));
        // a limit beyond the natural end changes nothing
        let far = Some(d(2024, 2, 1));
        let range = buffer(None, 5)
            .place(d(2024, 1, 1), &calendar(), far)
            .unwrap();
        assert_eq!(range.end(), d(2024, 1, 6));
    }

    #[test]
    fn empty_buffers_and_short_calendars() {
        let empty = buffer(None, 0);
        assert_eq!(
            placed(&empty, d(2024, 1, 3)),
            (d(2024, 1, 3), d(2024, 1, 3))
        );
        let long = buffer(None, 100);
        assert_eq!(long.place(d(2024, 1, 3), &calendar(), None), None);
        let set_outside = buffer(Some(d(2030, 1, 1)), 5);
        assert_eq!(set_outside.place(d(2024, 1, 3), &calendar(), None), None);
    }
}
