// modules{{{
use chrono::{Datelike, NaiveDate};

use crate::availability::{
    day_value::DayValue,
    rule::{DEFAULT_RULE, Rule, RuleKind, WeeklySchedule},
    rule_set::RuleSet,
};
// }}}

/// Precomputed from a rule set so the scheduler reads days by index instead of re-reading rules
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

    /// Inclusive
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
    pub fn calendar(&self, from: NaiveDate, to: NaiveDate) -> Calendar<V> {
        let len = ((to - from).num_days() + 1).max(0) as usize;
        let mut calendar = Calendar {
            start: from,
            values: vec![V::default(); len],
        };
        let default_week = match self.rules().get(&DEFAULT_RULE).map(Rule::kind) {
            Some(RuleKind::Weekly(week)) => week.clone(),
            _ => WeeklySchedule::default(),
        };
        for (kind, rules) in self.by_priority() {
            for rule in rules {
                let stop = rule
                    .stop_day()
                    .map_or(to, |s| s.pred_opt().unwrap_or(s).min(to));
                calendar.for_days(rule.start_day(), stop, |day, slot| {
                    match (kind, rule.kind()) {
                        (_, RuleKind::Weekly(week)) => *slot = week.on(day.weekday()),
                        (_, RuleKind::ResetWeekly) => *slot = default_week.on(day.weekday()),
                        (_, RuleKind::Override(o)) if o.sets(rule.start_day(), day) => {
                            *slot = o.value
                        }
                        (_, RuleKind::Holiday) => *slot = V::default(),
                        _ => {}
                    }
                });
            }
        }
        calendar
    }

    /// Later kinds override earlier ones; within a kind, by start day
    fn by_priority(&self) -> [(u8, Vec<&Rule<V>>); 3] {
        let of = |rank: u8| {
            let mut rules: Vec<&Rule<V>> = self
                .rules()
                .values()
                .filter(|r| rank_of(r.kind()) == rank)
                .collect();
            rules.sort_by_key(|r| (r.start_day(), *r.id()));
            (rank, rules)
        };
        [of(0), of(1), of(2)]
    }
}
// }}}

// rank_of {{{
pub(super) fn rank_of<V>(kind: &RuleKind<V>) -> u8 {
    match kind {
        RuleKind::Weekly(_) | RuleKind::ResetWeekly => 0,
        RuleKind::Override(_) => 1,
        RuleKind::Holiday => 2,
    }
}
// }}}
