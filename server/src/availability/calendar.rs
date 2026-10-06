// modules{{{
use chrono::{Datelike, Months, NaiveDate};

use crate::availability::{
    day_value::DayValue,
    rule::{Rule, RuleKind, WeeklySchedule},
    rule_set::RuleSet,
};
// }}}

/// Availability for a run of consecutive days, built once from a rule set so the scheduler can
/// read days by index instead of re-reading the rules
#[derive(Clone, Debug, PartialEq)]
pub struct Calendar<V> {
    start: NaiveDate,
    values: Vec<V>,
}

// impl Calendar{{{
impl<V: DayValue> Calendar<V> {
    pub fn start(&self) -> NaiveDate {
        self.start
    }

    /// The last day covered
    pub fn end(&self) -> NaiveDate {
        self.start + chrono::Days::new(self.values.len().saturating_sub(1) as u64)
    }

    pub fn covers(&self, from: NaiveDate, until: NaiveDate) -> bool {
        self.start <= from && until <= self.end()
    }

    pub fn on(&self, day: NaiveDate) -> Option<V> {
        let offset = usize::try_from((day - self.start).num_days()).ok()?;
        self.values.get(offset).copied()
    }

    pub fn values(&self) -> &[V] {
        &self.values
    }

    fn slice(&self, from: NaiveDate, to: NaiveDate) -> Self {
        let offset = (from - self.start).num_days() as usize;
        let len = (to - from).num_days() as usize + 1;
        Self {
            start: from,
            values: self.values[offset..offset + len].to_vec(),
        }
    }

    /// Runs `f` on each day of `from..=to` that falls inside the calendar, with its hours slot
    fn for_days(&mut self, from: NaiveDate, to: NaiveDate, mut f: impl FnMut(NaiveDate, &mut V)) {
        let (start, len) = (self.start, self.values.len());
        let first = from.max(start);
        for day in first.iter_days().take_while(|d| *d <= to) {
            match usize::try_from((day - start).num_days()) {
                Ok(i) if i < len => f(day, &mut self.values[i]),
                _ => break,
            }
        }
    }
}
// }}}

// impl RuleSet{{{
impl<V: DayValue> RuleSet<V> {
    /// Availability for every day in `from..=to`, applying each kind of rule in priority order
    /// across the whole range at once
    pub fn calendar(&self, from: NaiveDate, to: NaiveDate) -> Calendar<V> {
        if to < from {
            return Calendar {
                start: from,
                values: Vec::new(),
            };
        }
        let driving = self.driving_calendar(self.lookback(from), to);
        let mut calendar = driving.clone();

        let mut runs: Vec<_> = self.rules_of(|k| matches!(k, RuleKind::FiniteWorkdays { .. }));
        runs.sort_by_key(|r| (r.start_day(), *r.id()));
        for rule in runs {
            let RuleKind::FiniteWorkdays { value, .. } = rule.kind() else {
                continue;
            };
            for day in run_days(rule, &driving, to) {
                calendar.for_days(day, day, |_, slot| *slot = *value);
            }
        }
        for rule in self.rules_of(|k| matches!(k, RuleKind::Holiday)) {
            let stop = rule
                .stop_day()
                .map_or(to, |s| s.pred_opt().unwrap_or(s).min(to));
            calendar.for_days(rule.start_day(), stop, |_, slot| *slot = V::default());
        }
        calendar.slice(from, to)
    }

    /// The earliest day needed to evaluate `from`, since a run of workdays counts from its start
    pub(super) fn lookback(&self, from: NaiveDate) -> NaiveDate {
        self.rules_of(|k| matches!(k, RuleKind::FiniteWorkdays { .. }))
            .iter()
            .map(|r| r.start_day())
            .fold(from, NaiveDate::min)
    }

    /// Hours under the weekly and monthly rules alone
    pub(super) fn driving_calendar(&self, from: NaiveDate, to: NaiveDate) -> Calendar<V> {
        let len = ((to - from).num_days() + 1).max(0) as usize;
        let mut calendar = Calendar {
            start: from,
            values: vec![V::default(); len],
        };
        let default = WeeklySchedule::<V>::default();
        calendar.for_days(from, to, |day, slot| *slot = default.on(day.weekday()));

        let mut weekly = self.rules_of(|k| matches!(k, RuleKind::Weekly(_)));
        weekly.sort_by_key(|r| (r.start_day(), *r.id()));
        for rule in weekly {
            let RuleKind::Weekly(schedule) = rule.kind() else {
                continue;
            };
            let stop = rule
                .stop_day()
                .map_or(to, |s| s.pred_opt().unwrap_or(s).min(to));
            calendar.for_days(rule.start_day(), stop, |day, slot| {
                *slot = schedule.on(day.weekday())
            });
        }

        let mut monthly = self.rules_of(|k| matches!(k, RuleKind::Monthly(_)));
        monthly.sort_by_key(|r| match r.kind() {
            RuleKind::Monthly(m) => (m.priority, r.start_day(), *r.id()),
            _ => unreachable!("filtered to monthly rules"),
        });
        let first_month = from.with_day(1).expect("every month has a first day");
        let months: Vec<_> = (0..)
            .map_while(|n| first_month.checked_add_months(Months::new(n)))
            .take_while(|m| *m <= to)
            .collect();
        for rule in monthly {
            let RuleKind::Monthly(m) = rule.kind() else {
                continue;
            };
            for month in &months {
                for day in m.days_in_month(*month, self.week_start()) {
                    if rule.is_in_effect(day) {
                        calendar.for_days(day, day, |_, slot| *slot = m.value);
                    }
                }
            }
        }
        calendar
    }

    fn rules_of(&self, kind: impl Fn(&RuleKind<V>) -> bool) -> Vec<&Rule<V>> {
        self.rules().values().filter(|r| kind(r.kind())).collect()
    }
}
// }}}

/// The days a run of workdays sets up to `to`: the first `days` days from its start that are
/// working days (at least 1h) under `driving`, within the rule's stop day
pub(super) fn run_days<V: DayValue>(
    rule: &Rule<V>,
    driving: &Calendar<V>,
    to: NaiveDate,
) -> Vec<NaiveDate> {
    let RuleKind::FiniteWorkdays { days, .. } = rule.kind() else {
        return Vec::new();
    };
    rule.start_day()
        .iter_days()
        .take_while(|d| *d <= to && *d <= driving.end() && rule.is_in_effect(*d))
        .filter(|d| driving.on(*d).is_some_and(|v| v.is_working()))
        .take(*days as usize)
        .collect()
}
