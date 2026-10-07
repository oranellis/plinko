// modules{{{
use crate::helpers::double_option;
use chrono::{NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::day_value::DayValue,
    helpers::functions::check_id_match,
    traits::patch::Patchable,
    types::{error::PlinkoResult, workload::Workload},
};
// }}}

/// Rules may only start within this range, which keeps the calendars built from them a sane size
pub const RULE_START_RANGE: std::ops::RangeInclusive<NaiveDate> =
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()..=NaiveDate::from_ymd_opt(2100, 12, 31).unwrap();

/// Something that sets availability from `start_day` going forward until `stop_day`
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule<V> {
    id: Uuid,
    start_day: NaiveDate,
    /// The first day the rule no longer applies
    stop_day: Option<NaiveDate>,
    kind: RuleKind<V>,
}

// impl Rule{{{
impl<V> Rule<V> {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn start_day(&self) -> NaiveDate {
        self.start_day
    }

    pub fn stop_day(&self) -> Option<NaiveDate> {
        self.stop_day
    }

    pub fn kind(&self) -> &RuleKind<V> {
        &self.kind
    }

    /// Whether the rule has started and not been stopped on `day`
    pub fn is_in_effect(&self, day: NaiveDate) -> bool {
        self.start_day <= day && self.stop_day.is_none_or(|stop| day < stop)
    }
}

impl<V> From<NewRule<V>> for Rule<V> {
    fn from(new: NewRule<V>) -> Self {
        Self {
            id: new.id,
            start_day: new.start_day,
            stop_day: None,
            kind: new.kind,
        }
    }
}

impl<V> Patchable<RulePatch<V>> for Rule<V> {
    fn apply_patch(&mut self, patch: RulePatch<V>) -> PlinkoResult<()> {
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RuleKind<V> {
    /// Replaces the weekly schedule
    Weekly(WeeklySchedule<V>),
    Monthly(MonthlyOverride<V>),
    /// Sets the next `days` days that are working days under the weekly and monthly rules to
    /// `value`, leaving the days between untouched
    FiniteWorkdays {
        days: u32,
        value: V,
    },
    /// Sets the default value: no availability, or no days allowed
    Holiday,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeeklySchedule<V>([V; 7]);

// impl WeeklySchedule{{{
impl<V: DayValue> WeeklySchedule<V> {
    /// One value per day, Monday first
    pub fn new(values: [V; 7]) -> Self {
        Self(values)
    }

    pub fn on(&self, weekday: Weekday) -> V {
        self.0[weekday.num_days_from_monday() as usize]
    }
}

impl WeeklySchedule<Workload> {
    /// Hours per day, Monday first
    pub fn from_hours(hours: [u8; 7]) -> PlinkoResult<Self> {
        let [mon, tue, wed, thu, fri, sat, sun] = hours.map(Workload::new);
        Ok(Self([mon?, tue?, wed?, thu?, fri?, sat?, sun?]))
    }
}

impl<V: DayValue> Default for WeeklySchedule<V> {
    fn default() -> Self {
        Self(V::default_week())
    }
}
// }}}

/// Sets `value` on the days picked by `ordinal` and `target` in every month
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonthlyOverride<V> {
    pub ordinal: Ordinal,
    pub target: MonthTarget,
    pub value: V,
    /// The higher priority wins where monthly rules overlap, then the later start day
    pub priority: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonthTarget {
    /// The day of the month
    Day,
    /// The weekday of the month, like the second Wednesday
    Weekday(Weekday),
    /// A full week (starting on the rule set's week start, entirely in the month), with a toggle
    /// per weekday (Monday first) for which of its days are set
    Week([bool; 7]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct NewRule<V> {
    pub id: Uuid,
    pub start_day: NaiveDate,
    pub kind: RuleKind<V>,
}

#[derive(Serialize, Deserialize)]
pub struct RulePatch<V> {
    pub id: Uuid,
    pub start_day: Option<NaiveDate>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub stop_day: Option<Option<NaiveDate>>,
    pub kind: Option<RuleKind<V>>,
}
