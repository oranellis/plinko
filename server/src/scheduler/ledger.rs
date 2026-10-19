// modules {{{
use std::collections::HashMap;

use chrono::{Days, NaiveDate};
use uuid::Uuid;

use crate::{
    availability::calendar::Calendar, availability::workload::Workload,
    multitenancy::tenant::Tenant,
};
// }}}

// constants {{{
/// How far past the start of a search the scheduler looks for capacity
pub const MAX_FILL_DAYS: u32 = 3650;
// }}}

/// Remaining hours per person per day over a window of days, shared by every plan. Placeholders
/// are unlimited so have no entry. Days before the window are past and cost nothing, days after
/// it have no capacity.
#[derive(Clone, Debug, PartialEq)]
pub struct Ledger {
    start: NaiveDate,
    days: usize,
    capacity: HashMap<Uuid, Vec<u8>>,
    free: HashMap<Uuid, Vec<u8>>,
}

// Ledger supporting types {{{
/// Hours of one person on one day
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claim {
    pub user: Uuid,
    pub day: NaiveDate,
    pub hours: u8,
}

/// The claims of one placement, so it can be taken and given back as a whole
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run(pub Vec<Claim>);
// }}}

// impl Ledger {{{
impl Ledger {
    /// A ledger for `days` days from `start`, with each person's hours from their calendar (none
    /// on days it does not cover)
    pub fn new<'a>(
        start: NaiveDate,
        days: usize,
        people: impl IntoIterator<Item = (Uuid, &'a Calendar<Workload>)>,
    ) -> Self {
        let capacity: HashMap<_, _> = people
            .into_iter()
            .map(|(user, calendar)| {
                let hours = (0..days)
                    .map(|i| start + Days::new(i as u64))
                    .map(|day| calendar.on(day).map_or(0, |w| w.get()))
                    .collect();
                (user, hours)
            })
            .collect();
        Self {
            start,
            days,
            free: capacity.clone(),
            capacity,
        }
    }

    /// Every tenant user over `MAX_FILL_DAYS` from `start`, from the calendars cached by
    /// `Tenant::refresh_calendars`
    pub fn from_tenant(tenant: &Tenant, start: NaiveDate) -> Self {
        let people = tenant
            .users()
            .keys()
            .filter_map(|u| Some((*u, tenant.calendar(u)?)));
        Self::new(start, MAX_FILL_DAYS as usize, people)
    }

    pub fn start(&self) -> NaiveDate {
        self.start
    }

    /// The first day after the window
    pub fn end(&self) -> NaiveDate {
        self.start + Days::new(self.days as u64)
    }

    fn index(&self, day: NaiveDate) -> Option<usize> {
        usize::try_from((day - self.start).num_days())
            .ok()
            .filter(|i| *i < self.days)
    }

    fn hours(&self, hours: &HashMap<Uuid, Vec<u8>>, user: &Uuid, day: NaiveDate) -> u8 {
        let hours = hours.get(user);
        self.index(day)
            .and_then(|i| hours?.get(i))
            .copied()
            .unwrap_or(0)
    }

    /// The person's hours that day before anything is claimed
    pub fn capacity(&self, user: &Uuid, day: NaiveDate) -> u8 {
        self.hours(&self.capacity, user, day)
    }

    /// Hours still unclaimed
    pub fn free(&self, user: &Uuid, day: NaiveDate) -> u8 {
        self.hours(&self.free, user, day)
    }

    /// Takes every claim in the run, or none of them if any day lacks the hours. Claims on days
    /// before the window are accepted and ignored.
    pub fn claim(&mut self, run: &Run) -> bool {
        for (i, claim) in run.0.iter().enumerate() {
            if !self.take(claim) {
                self.release(&Run(run.0[..i].to_vec()));
                return false;
            }
        }
        true
    }

    /// Gives back a run that was claimed
    pub fn release(&mut self, run: &Run) {
        for claim in &run.0 {
            let slot = self.index(claim.day).and_then(|i| {
                let capacity = self.capacity.get(&claim.user)?.get(i)?;
                Some((self.free.get_mut(&claim.user)?.get_mut(i)?, *capacity))
            });
            if let Some((free, capacity)) = slot {
                *free = free.saturating_add(claim.hours).min(capacity);
            }
        }
    }

    fn take(&mut self, claim: &Claim) -> bool {
        let Some(i) = self.index(claim.day) else {
            return claim.day < self.start || claim.hours == 0;
        };
        match self.free.get_mut(&claim.user).and_then(|f| f.get_mut(i)) {
            Some(free) if *free >= claim.hours => {
                *free -= claim.hours;
                true
            }
            _ => claim.hours == 0,
        }
    }

    /// Booked hours as a share of capacity from the person's first to last booked day, 0 if
    /// they have nothing booked
    pub fn load(&self, user: &Uuid) -> f32 {
        let (Some(capacity), Some(free)) = (self.capacity.get(user), self.free.get(user)) else {
            return 0.0;
        };
        let booked = |i: usize| capacity[i] - free[i];
        let mut days = (0..self.days).filter(|i| booked(*i) > 0);
        let (Some(first), Some(last)) = (days.clone().next(), days.next_back()) else {
            return 0.0;
        };
        let booked: u32 = (first..=last).map(|i| u32::from(booked(i))).sum();
        let available: u32 = capacity[first..=last].iter().map(|h| u32::from(*h)).sum();
        booked as f32 / available as f32
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::rule_set::RuleSet;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn claim(user: Uuid, day: u32, hours: u8) -> Claim {
        Claim {
            user,
            day: d(day),
            hours,
        }
    }

    /// One person with default weekdays from Monday the 1st, over two weeks
    fn ledger(user: Uuid) -> Ledger {
        let calendar = RuleSet::default().calendar(d(1), d(14));
        Ledger::new(d(1), 14, [(user, &calendar)])
    }

    #[test]
    fn capacity_comes_from_the_calendar_and_is_zero_outside_it() {
        let user = Uuid::now_v7();
        let ledger = ledger(user);
        assert_eq!(ledger.free(&user, d(1)), 8);
        assert_eq!(ledger.free(&user, d(6)), 0, "Saturday");
        assert_eq!(ledger.free(&user, d(15)), 0, "after the window");
        assert_eq!(ledger.free(&user, NaiveDate::MIN), 0, "before the window");
        assert_eq!(ledger.free(&Uuid::now_v7(), d(1)), 0, "unknown person");
    }

    #[test]
    fn claims_take_hours_and_releases_return_them() {
        let user = Uuid::now_v7();
        let mut ledger = ledger(user);
        let run = Run(vec![claim(user, 1, 5), claim(user, 2, 8)]);
        assert!(ledger.claim(&run));
        assert_eq!((ledger.free(&user, d(1)), ledger.free(&user, d(2))), (3, 0));
        assert_eq!(ledger.capacity(&user, d(1)), 8);
        ledger.release(&run);
        assert_eq!((ledger.free(&user, d(1)), ledger.free(&user, d(2))), (8, 8));
    }

    #[test]
    fn a_failed_run_takes_nothing() {
        let user = Uuid::now_v7();
        let mut ledger = ledger(user);
        let before = ledger.clone();
        // the second claim fits alone, the third overdraws the first day, or hits a weekend
        for last in [claim(user, 1, 4), claim(user, 6, 1), claim(user, 20, 1)] {
            let run = Run(vec![claim(user, 1, 6), claim(user, 2, 3), last]);
            assert!(!ledger.claim(&run));
            assert_eq!(ledger, before);
        }
    }

    #[test]
    fn past_days_are_ignored_and_zero_hour_claims_always_fit() {
        let user = Uuid::now_v7();
        let mut ledger = ledger(user);
        let before = ledger.clone();
        let run = Run(vec![
            claim(user, 1, 0),
            claim(user, 6, 0),
            claim(user, 20, 0),
        ]);
        assert!(ledger.claim(&run));
        let past = NaiveDate::from_ymd_opt(2023, 12, 25).unwrap();
        let run = Run(vec![Claim {
            user,
            day: past,
            hours: 8,
        }]);
        assert!(ledger.claim(&run));
        ledger.release(&run);
        assert_eq!(ledger, before);
    }

    #[test]
    fn releases_never_exceed_capacity() {
        let user = Uuid::now_v7();
        let mut ledger = ledger(user);
        ledger.release(&Run(vec![claim(user, 1, 8)]));
        assert_eq!(ledger.free(&user, d(1)), 8);
    }

    #[test]
    fn load_is_booked_over_available_between_first_and_last_booked_day() {
        let user = Uuid::now_v7();
        let mut ledger = ledger(user);
        assert_eq!(ledger.load(&user), 0.0);
        // Friday 4h and Monday 8h: 12 of the 16 hours across the two working days
        assert!(ledger.claim(&Run(vec![claim(user, 5, 4), claim(user, 8, 8)])));
        assert_eq!(ledger.load(&user), 0.75);
        assert_eq!(ledger.load(&Uuid::now_v7()), 0.0);
    }
}
// }}}
