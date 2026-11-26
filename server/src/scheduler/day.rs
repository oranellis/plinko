// modules {{{
use chrono::{Days, NaiveDate};
// }}}

/// A day as an offset from the origin of a run, so the scheduler indexes rows instead of doing
/// date arithmetic. As the time of a vertex it is the boundary before that day: an arc over
/// `[start, end)` works the days `start..end`, and what follows it can start on `end`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Day(pub u32);

// impl Day {{{
impl Day {
    /// The day of `date`, the origin for dates before it
    pub fn of(date: NaiveDate, origin: NaiveDate) -> Self {
        Day(u32::try_from((date - origin).num_days()).unwrap_or(0))
    }

    pub fn date(self, origin: NaiveDate) -> NaiveDate {
        origin + Days::new(u64::from(self.0))
    }

    pub fn next(self) -> Self {
        Day(self.0 + 1)
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
}
// }}}
