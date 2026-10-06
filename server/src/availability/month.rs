// modules{{{
use chrono::{Datelike, Days, NaiveDate, Weekday};

use crate::availability::rule::{MonthTarget, MonthlyOverride, Ordinal};
// }}}

/// Picks the item an ordinal points at. An ordinal beyond the end (a fifth Monday in a month
/// with four) falls back to the nearest valid one, the last item counting in the same direction.
fn pick<T: Copy>(items: &[T], ordinal: Ordinal) -> Option<T> {
    let (index, from_end) = ordinal.index();
    let index = index.min(items.len()).checked_sub(1)?;
    match from_end {
        false => items.get(index),
        true => items.iter().rev().nth(index),
    }
    .copied()
}

fn days_of_month(day: NaiveDate) -> Vec<NaiveDate> {
    let first = day.with_day(1).expect("every month has a first day");
    first
        .iter_days()
        .take_while(|d| d.month() == first.month())
        .collect()
}

impl<V> MonthlyOverride<V> {
    /// The days this rule picks in the month containing `month_of`. `week_start` decides which
    /// weeks are full for the `Week` target.
    pub fn days_in_month(&self, month_of: NaiveDate, week_start: Weekday) -> Vec<NaiveDate> {
        let days = days_of_month(month_of);
        match self.target {
            MonthTarget::Day => pick(&days, self.ordinal).into_iter().collect(),
            MonthTarget::Weekday(weekday) => {
                let days: Vec<_> = days
                    .into_iter()
                    .filter(|d| d.weekday() == weekday)
                    .collect();
                pick(&days, self.ordinal).into_iter().collect()
            }
            MonthTarget::Week(toggled) => {
                let last = days[days.len() - 1];
                let weeks: Vec<_> = days
                    .into_iter()
                    .filter(|d| d.weekday() == week_start && *d + Days::new(6) <= last)
                    .collect();
                let Some(week) = pick(&weeks, self.ordinal) else {
                    return Vec::new();
                };
                week.iter_days()
                    .take(7)
                    .filter(|d| toggled[d.weekday().num_days_from_monday() as usize])
                    .collect()
            }
        }
    }
}
