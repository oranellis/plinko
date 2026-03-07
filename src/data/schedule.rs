//! Work schedule types — [`Weekday`] and [`WorkSchedule`].

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Converts a [`chrono::Weekday`] to the project's own [`Weekday`] enum.
pub fn chrono_to_weekday(d: chrono::Weekday) -> Weekday {
    match d {
        chrono::Weekday::Mon => Weekday::Monday,
        chrono::Weekday::Tue => Weekday::Tuesday,
        chrono::Weekday::Wed => Weekday::Wednesday,
        chrono::Weekday::Thu => Weekday::Thursday,
        chrono::Weekday::Fri => Weekday::Friday,
        chrono::Weekday::Sat => Weekday::Saturday,
        chrono::Weekday::Sun => Weekday::Sunday,
    }
}

/// Day-of-week identifier used as a key in [`WorkSchedule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// Hours worked per day. Days absent from the map are non-working days.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSchedule {
    pub days: HashMap<Weekday, f32>,
}

impl WorkSchedule {
    /// Mon–Fri, 8 hours each (the default).
    pub fn weekdays() -> Self {
        Self {
            days: [
                (Weekday::Monday, 8.0),
                (Weekday::Tuesday, 8.0),
                (Weekday::Wednesday, 8.0),
                (Weekday::Thursday, 8.0),
                (Weekday::Friday, 8.0),
            ]
            .into_iter()
            .collect(),
        }
    }

    /// All 7 days, 8 hours each.
    pub fn full_week() -> Self {
        let mut s = Self::weekdays();
        s.days.insert(Weekday::Saturday, 8.0);
        s.days.insert(Weekday::Sunday, 8.0);
        s
    }

    /// Add or update a day with the given hours. Can be chained: `WorkSchedule::weekdays().with_day(Weekday::Saturday, 8.0)`.
    pub fn with_day(mut self, day: Weekday, hours: f32) -> Self {
        self.days.insert(day, hours);
        self
    }

    /// Remove a day, making it a non-working day. Can be chained.
    pub fn without_day(mut self, day: Weekday) -> Self {
        self.days.remove(&day);
        self
    }

    /// Returns `true` if `day` has a non-zero entry in this schedule.
    pub fn is_working_day(&self, day: Weekday) -> bool {
        self.days.contains_key(&day)
    }

    /// Hours scheduled on `day`, or `0.0` if the day is not in the schedule.
    pub fn hours_on(&self, day: Weekday) -> f32 {
        self.days.get(&day).copied().unwrap_or(0.0)
    }

    /// Sum of hours across all days in the schedule.
    pub fn total_hours_per_week(&self) -> f32 {
        self.days.values().sum()
    }

    /// Total working days per week (counts any day with hours > 0).
    pub fn working_days_per_week(&self) -> f32 {
        self.days.values().filter(|&&h| h > 0.0).count() as f32
    }
}

impl Default for WorkSchedule {
    fn default() -> Self {
        Self::weekdays()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays_has_mon_to_fri_only() {
        let s = WorkSchedule::weekdays();
        assert!(s.is_working_day(Weekday::Monday));
        assert!(s.is_working_day(Weekday::Tuesday));
        assert!(s.is_working_day(Weekday::Wednesday));
        assert!(s.is_working_day(Weekday::Thursday));
        assert!(s.is_working_day(Weekday::Friday));
        assert!(!s.is_working_day(Weekday::Saturday));
        assert!(!s.is_working_day(Weekday::Sunday));
    }

    #[test]
    fn weekdays_is_eight_hours_per_day() {
        let s = WorkSchedule::weekdays();
        for day in [Weekday::Monday, Weekday::Tuesday, Weekday::Wednesday, Weekday::Thursday, Weekday::Friday] {
            assert_eq!(s.hours_on(day), 8.0);
        }
    }

    #[test]
    fn weekdays_total_is_40h() {
        assert_eq!(WorkSchedule::weekdays().total_hours_per_week(), 40.0);
    }

    #[test]
    fn weekdays_working_days_per_week_is_5() {
        assert_eq!(WorkSchedule::weekdays().working_days_per_week(), 5.0);
    }

    #[test]
    fn full_week_includes_weekend() {
        let s = WorkSchedule::full_week();
        assert!(s.is_working_day(Weekday::Saturday));
        assert!(s.is_working_day(Weekday::Sunday));
        assert_eq!(s.total_hours_per_week(), 56.0);
        assert_eq!(s.working_days_per_week(), 7.0);
    }

    #[test]
    fn with_day_adds_and_updates() {
        let s = WorkSchedule::weekdays().with_day(Weekday::Saturday, 4.0);
        assert!(s.is_working_day(Weekday::Saturday));
        assert_eq!(s.hours_on(Weekday::Saturday), 4.0);
        // Update existing day
        let s2 = s.with_day(Weekday::Monday, 6.0);
        assert_eq!(s2.hours_on(Weekday::Monday), 6.0);
    }

    #[test]
    fn without_day_removes_day() {
        let s = WorkSchedule::weekdays().without_day(Weekday::Friday);
        assert!(!s.is_working_day(Weekday::Friday));
        assert_eq!(s.hours_on(Weekday::Friday), 0.0);
        assert_eq!(s.working_days_per_week(), 4.0);
    }

    #[test]
    fn hours_on_absent_day_is_zero() {
        let s = WorkSchedule::weekdays();
        assert_eq!(s.hours_on(Weekday::Saturday), 0.0);
        assert_eq!(s.hours_on(Weekday::Sunday), 0.0);
    }

    #[test]
    fn default_is_weekdays() {
        let s = WorkSchedule::default();
        assert_eq!(s.total_hours_per_week(), 40.0);
        assert!(!s.is_working_day(Weekday::Saturday));
    }

    #[test]
    fn chrono_weekday_conversion() {
        assert_eq!(chrono_to_weekday(chrono::Weekday::Mon), Weekday::Monday);
        assert_eq!(chrono_to_weekday(chrono::Weekday::Sat), Weekday::Saturday);
        assert_eq!(chrono_to_weekday(chrono::Weekday::Sun), Weekday::Sunday);
    }
}
