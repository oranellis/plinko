//! Date-specific hour overrides used to model holidays, half-days, and other exceptions.

use std::collections::HashMap;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A set of specific date exceptions that override the normal work schedule.
/// For example, a bank holiday (0h) or a half-day (4h).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalendarOverrides {
    /// Map of date → hours available on that date.
    /// A value of 0.0 means the day is completely off.
    /// Dates absent from the map are not overridden.
    pub entries: HashMap<NaiveDate, f32>,
}

impl CalendarOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the available hours for a specific date.
    pub fn set(&mut self, date: NaiveDate, hours: f32) {
        self.entries.insert(date, hours);
    }

    /// Remove an override, reverting the date to the normal schedule.
    pub fn remove(&mut self, date: &NaiveDate) {
        self.entries.remove(date);
    }

    /// Returns `Some(hours)` if this date has an override, `None` if it follows the normal schedule.
    pub fn get(&self, date: NaiveDate) -> Option<f32> {
        self.entries.get(&date).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn new_is_empty() {
        let c = CalendarOverrides::new();
        assert!(c.entries.is_empty());
    }

    #[test]
    fn set_and_get() {
        let mut c = CalendarOverrides::new();
        c.set(date(2026, 3, 9), 3.0);
        assert_eq!(c.get(date(2026, 3, 9)), Some(3.0));
    }

    #[test]
    fn get_absent_date_is_none() {
        let c = CalendarOverrides::new();
        assert_eq!(c.get(date(2026, 3, 9)), None);
    }

    #[test]
    fn set_zero_means_day_off() {
        let mut c = CalendarOverrides::new();
        c.set(date(2026, 3, 9), 0.0);
        assert_eq!(c.get(date(2026, 3, 9)), Some(0.0));
    }

    #[test]
    fn remove_reverts_to_none() {
        let mut c = CalendarOverrides::new();
        let d = date(2026, 3, 9);
        c.set(d, 4.0);
        c.remove(&d);
        assert_eq!(c.get(d), None);
    }

    #[test]
    fn overwriting_a_date_updates_value() {
        let mut c = CalendarOverrides::new();
        let d = date(2026, 3, 9);
        c.set(d, 4.0);
        c.set(d, 6.0);
        assert_eq!(c.get(d), Some(6.0));
    }
}
