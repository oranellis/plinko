// modules{{{
use crate::helpers::double_option;
use chrono::{Datelike, Days, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::day_value::DayValue, availability::workload::Workload,
    common::error::PlinkoResult, helpers::functions::check_id_match, traits::patch::Patchable,
};
// }}}

// constants {{{
/// Bounded so the calendars built from rules stay a sane size
pub const RULE_START_RANGE: std::ops::RangeInclusive<NaiveDate> =
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()..=NaiveDate::from_ymd_opt(2100, 12, 31).unwrap();

pub const MAX_NAME: usize = 60;

/// The weekly schedule every rule set starts with, so there is always one to change. It cannot be
/// removed or moved.
pub const DEFAULT_RULE: Uuid = Uuid::max();

/// Deliberately outside `RULE_START_RANGE`
pub const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
// }}}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule<V> {
    id: Uuid,
    start_day: NaiveDate,
    /// Exclusive: the first day the rule no longer applies
    stop_day: Option<NaiveDate>,
    kind: RuleKind<V>,
    /// Empty to go by its kind
    name: String,
}

// impl Rule{{{
impl<V: DayValue> Rule<V> {
    pub(super) fn default_weekly() -> Self {
        Self {
            id: DEFAULT_RULE,
            start_day: EPOCH,
            stop_day: None,
            kind: RuleKind::Weekly(WeeklySchedule::default()),
            name: String::new(),
        }
    }
}

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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_in_effect(&self, day: NaiveDate) -> bool {
        self.start_day <= day && self.stop_day.is_none_or(|stop| day < stop)
    }

    /// Unlike `is_in_effect`, an override only applies on the days it sets
    pub fn applies_on(&self, day: NaiveDate) -> bool {
        self.is_in_effect(day)
            && match &self.kind {
                RuleKind::Override(o) => o.sets(self.start_day, day),
                _ => true,
            }
    }
}

impl<V> From<NewRule<V>> for Rule<V> {
    fn from(new: NewRule<V>) -> Self {
        Self {
            id: new.id,
            start_day: new.start_day,
            stop_day: new.stop_day,
            kind: new.kind,
            name: new.name,
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
        if let Some(n) = patch.name {
            self.name = n;
        }
        Ok(())
    }
}
// }}}

// Rule supporting types {{{
/// In increasing priority: a weekly schedule, overrides of single days or runs of them, and
/// holidays. A rule that starts later wins over an earlier one of its kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RuleKind<V> {
    Weekly(WeeklySchedule<V>),
    Override(DayOverride<V>),
    /// Sets the default value (no availability, or no days allowed). Must have a stop day.
    Holiday,
    /// Back to the default weekly schedule until another weekly schedule
    ResetWeekly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeeklySchedule<V>([V; 7]);

// impl WeeklySchedule{{{
impl<V: DayValue> WeeklySchedule<V> {
    /// Monday first
    pub fn new(values: [V; 7]) -> Self {
        Self(values)
    }

    pub fn on(&self, weekday: Weekday) -> V {
        self.0[weekday.num_days_from_monday() as usize]
    }
}

impl WeeklySchedule<Workload> {
    /// Monday first
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

/// Sets `value` on `days` calendar days from the rule's start day, repeating every `every` (once
/// without it). The rule's stop day cuts occurrences off.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DayOverride<V> {
    pub days: u32,
    pub value: V,
    pub every: Option<Every>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Every {
    pub n: u32,
    pub unit: Unit,
    /// For aligned months: count weekdays from the end of the month
    #[serde(default)]
    pub from_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Day,
    Week,
    Month,
    /// By weekday, e.g. the second Monday of each month, rather than the same day number
    MonthAligned,
    Year,
}

// impl Every {{{
impl Every {
    /// The first day of the latest occurrence starting on or before `day`. Months and years keep the
    /// day of the month, clamped to a shorter month's last day. Aligned months keep `start`'s
    /// weekday index (from the start or end), falling back to the month's last such weekday.
    fn occurrence(&self, start: NaiveDate, day: NaiveDate) -> Option<NaiveDate> {
        let n = i64::from(self.n.max(1));
        let elapsed = (day - start).num_days();
        if elapsed < 0 {
            return None;
        }
        let step = match self.unit {
            Unit::Day => return Some(start + Days::new((elapsed / n) as u64 * n as u64)),
            Unit::Week => {
                return Some(start + Days::new((elapsed / (7 * n)) as u64 * 7 * n as u64));
            }
            Unit::Year => 12 * n,
            Unit::Month | Unit::MonthAligned => n,
        };
        let months = i64::from(day.year() - start.year()) * 12 + i64::from(day.month0())
            - i64::from(start.month0());
        // an occurrence can fall after `day` in the same month, so the guess may be one too many
        let guess = (months / step).max(0);
        [guess, guess - 1]
            .into_iter()
            .filter(|k| *k >= 0)
            .filter_map(|k| self.in_month(start, (k * step) as u32))
            .find(|occurrence| *occurrence <= day)
    }

    fn in_month(&self, start: NaiveDate, months: u32) -> Option<NaiveDate> {
        if self.unit != Unit::MonthAligned {
            return start.checked_add_months(Months::new(months));
        }
        let first = start.with_day(1)?.checked_add_months(Months::new(months))?;
        let last_day = |month: NaiveDate| {
            month
                .checked_add_months(Months::new(1))
                .and_then(|next| next.pred_opt())
                .map_or(28, |d| d.day())
        };
        let length = last_day(start.with_day(1)?);
        let (weekday, length_here) = (start.weekday(), last_day(first));
        let from_start = |index: u32| {
            let offset =
                (weekday.num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
            let mut day = 1 + offset + 7 * index;
            while day > length_here {
                day -= 7;
            }
            day
        };
        let from_end = |index: u32| {
            let end_weekday = (first.weekday().num_days_from_monday() + length_here - 1) % 7;
            let offset = (end_weekday + 7 - weekday.num_days_from_monday()) % 7;
            let mut day = i64::from(length_here) - i64::from(offset) - 7 * i64::from(index);
            while day < 1 {
                day += 7;
            }
            day as u32
        };
        let day = if self.from_end {
            from_end((length - start.day()) / 7)
        } else {
            from_start((start.day() - 1) / 7)
        };
        first.with_day(day)
    }
}
// }}}

// impl DayOverride {{{
impl<V> DayOverride<V> {
    pub fn sets(&self, start: NaiveDate, day: NaiveDate) -> bool {
        let first = match &self.every {
            None => Some(start),
            Some(every) => every.occurrence(start, day),
        };
        first.is_some_and(|first| day >= first && (day - first).num_days() < i64::from(self.days))
    }
}
// }}}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewRule<V> {
    pub id: Uuid,
    pub start_day: NaiveDate,
    /// Exclusive
    #[serde(default)]
    pub stop_day: Option<NaiveDate>,
    pub kind: RuleKind<V>,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
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
    pub name: Option<String>,
}
// }}}
