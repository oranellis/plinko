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

impl MonthlyOverride {
    /// Whether `day` is one of the days this rule picks in its month. `week_start` decides which
    /// weeks are full for the `Week` target.
    pub fn matches(&self, day: NaiveDate, week_start: Weekday) -> bool {
        let days = days_of_month(day);
        match self.target {
            MonthTarget::Day => pick(&days, self.ordinal) == Some(day),
            MonthTarget::Weekday(weekday) => {
                let days: Vec<_> = days
                    .into_iter()
                    .filter(|d| d.weekday() == weekday)
                    .collect();
                pick(&days, self.ordinal) == Some(day)
            }
            MonthTarget::Week(toggled) => {
                let last = days[days.len() - 1];
                let weeks: Vec<_> = days
                    .into_iter()
                    .filter(|d| d.weekday() == week_start && *d + Days::new(6) <= last)
                    .collect();
                let Some(week) = pick(&weeks, self.ordinal) else {
                    return false;
                };
                let offset = (day - week).num_days();
                (0..7).contains(&offset) && toggled[day.weekday().num_days_from_monday() as usize]
            }
        }
    }
}
