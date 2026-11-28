// modules {{{
use chrono::{Days, NaiveDate};
// }}}

/// Days since the origin of a run. As a vertex time it is the boundary before that day: an arc
/// over `[start, end)` works `start..end`, and what follows can start on `end`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Day(pub u32);

// impl Day {{{
impl Day {
    /// Dates before the origin clamp to it
    pub fn of(date: NaiveDate, origin: NaiveDate) -> Self {
        Day(u32::try_from((date - origin).num_days()).unwrap_or(0))
    }

    /// Days past the last date there is clamp to it
    pub fn date(self, origin: NaiveDate) -> NaiveDate {
        let date = origin.checked_add_days(Days::new(u64::from(self.0)));
        date.unwrap_or(NaiveDate::MAX)
    }

    pub fn next(self) -> Self {
        Day(self.0.saturating_add(1))
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_round_trip_and_clamp_before_the_origin() {
        let origin = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let later = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        assert_eq!(Day::of(later, origin), Day(31));
        assert_eq!(Day(31).date(origin), later);
        assert_eq!(Day::of(origin - Days::new(3), origin), Day(0));
    }

    #[test]
    fn days_past_the_last_date_stay_on_it() {
        let last = Day::of(NaiveDate::MAX, NaiveDate::MIN);
        assert_eq!(last.next().date(NaiveDate::MIN), NaiveDate::MAX);
        assert_eq!(Day(u32::MAX).next(), Day(u32::MAX));
    }
}
// }}}
