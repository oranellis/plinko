// modules{{{
use chrono::{NaiveDate, Weekday};
use uuid::Uuid;

use crate::{
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{error::PlinkoResult, workload::Workload},
};
// }}}

/// Something that sets availability from `start_day` going forward until `stop_day`
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    id: Uuid,
    start_day: NaiveDate,
    /// The first day the rule no longer applies
    stop_day: Option<NaiveDate>,
    kind: RuleKind,
}

// impl Rule{{{
impl Rule {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn start_day(&self) -> NaiveDate {
        self.start_day
    }

    pub fn stop_day(&self) -> Option<NaiveDate> {
        self.stop_day
    }

    pub fn kind(&self) -> &RuleKind {
        &self.kind
    }

    /// Whether the rule has started and not been stopped on `day`
    pub fn is_in_effect(&self, day: NaiveDate) -> bool {
        self.start_day <= day && self.stop_day.is_none_or(|stop| day < stop)
    }
}

impl From<NewRule> for Rule {
    fn from(new: NewRule) -> Self {
        Self {
            id: new.id,
            start_day: new.start_day,
            stop_day: None,
            kind: new.kind,
        }
    }
}

impl Patchable<RulePatch> for Rule {
    fn apply_patch(&mut self, patch: RulePatch) -> PlinkoResult<()> {
        check_id_match(self.id, patch.id)?;
        if let Some(s) = patch.start_day {
            self.start_day = s;
        }
        if let Some(s) = patch.stop_day {
            self.stop_day = s;
        }
        if let Some(k) = patch.kind {
            self.kind = k;
        }
        Ok(())
    }
}
// }}}

/// In increasing priority: a weekly schedule, special monthly days, a finite run of workdays, and
/// holidays.
#[derive(Clone, Debug, PartialEq)]
pub enum RuleKind {
    /// Replaces the weekly schedule
    Weekly(WeeklySchedule),
    Monthly(MonthlyOverride),
    /// Sets the next `days` days that are working days (at least 1h) under the weekly and monthly
    /// rules to `hours`, leaving the days between untouched
    FiniteWorkdays {
        days: u32,
        hours: Workload,
    },
    /// No availability
    Holiday,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeeklySchedule([Workload; 7]);

// impl WeeklySchedule{{{
impl WeeklySchedule {
    /// Hours per day, Monday first
    pub fn new(hours: [u8; 7]) -> PlinkoResult<Self> {
        let [mon, tue, wed, thu, fri, sat, sun] = hours.map(Workload::new);
        Ok(Self([mon?, tue?, wed?, thu?, fri?, sat?, sun?]))
    }

    pub fn on(&self, weekday: Weekday) -> Workload {
        self.0[weekday.num_days_from_monday() as usize]
    }
}

impl Default for WeeklySchedule {
    fn default() -> Self {
        Self::new([8, 8, 8, 8, 8, 0, 0]).expect("default hours are valid")
    }
}
// }}}

/// Sets `hours` on the days picked by `ordinal` and `target` in every month
#[derive(Clone, Debug, PartialEq)]
pub struct MonthlyOverride {
    pub ordinal: Ordinal,
    pub target: MonthTarget,
    pub hours: Workload,
    /// The higher priority wins where monthly rules overlap, then the later start day
    pub priority: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonthTarget {
    /// The day of the month
    Day,
    /// The weekday of the month, like the second Wednesday
    Weekday(Weekday),
    /// A full week (starting on the rule set's week start, entirely in the month), with a toggle
    /// per weekday (Monday first) for which of its days are set
    Week([bool; 7]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ordinal {
    First,
    Second,
    Third,
    Fourth,
    Fifth,
    Last,
    SecondLast,
    ThirdLast,
    FourthLast,
    FifthLast,
}

impl Ordinal {
    /// The 1 based position and whether it counts from the end
    pub fn index(self) -> (usize, bool) {
        match self {
            Ordinal::First => (1, false),
            Ordinal::Second => (2, false),
            Ordinal::Third => (3, false),
            Ordinal::Fourth => (4, false),
            Ordinal::Fifth => (5, false),
            Ordinal::Last => (1, true),
            Ordinal::SecondLast => (2, true),
            Ordinal::ThirdLast => (3, true),
            Ordinal::FourthLast => (4, true),
            Ordinal::FifthLast => (5, true),
        }
    }
}

pub struct NewRule {
    pub id: Uuid,
    pub start_day: NaiveDate,
    pub kind: RuleKind,
}

pub struct RulePatch {
    pub id: Uuid,
    pub start_day: Option<NaiveDate>,
    pub stop_day: Option<Option<NaiveDate>>,
    pub kind: Option<RuleKind>,
}
