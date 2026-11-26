// modules {{{
use crate::scheduler_new::{
    arc::ArcId,
    bits::{Bits, Valid},
    day::Day,
    table::{Ask, RowId, Table},
};
// }}}

/// The hours people still have free from today on, which work claims as it is placed. Days
/// before today are history and cost nothing.
#[derive(Clone, Debug)]
pub struct Ledger {
    today: Day,
    /// Free hours per day for each row people's hours are taken from, empty for the others
    free: Vec<Vec<u8>>,
    /// For each such row, the days with fewer than 1 to 8 hours free
    short: Vec<Vec<Bits>>,
    /// What holds hours of each row, as the span it holds them over
    held: Vec<Vec<(Day, Day, ArcId)>>,
}

// Ledger supporting types {{{
/// Where a run of days fits
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub start: Day,
    /// The day after the last
    pub end: Day,
    /// The last day other work held that pushed the window later, and whose hours it held
    pub blocker: Option<(RowId, Day)>,
}
// }}}

// impl Ledger {{{
impl Ledger {
    pub fn new(table: &Table) -> Self {
        let rows = (0..table.rows() as u32).map(RowId);
        let claimable = |row: &RowId| table.claimable(*row);
        let free: Vec<Vec<u8>> = rows
            .clone()
            .map(|row| match claimable(&row) {
                true => table.row_hours(row).to_vec(),
                false => Vec::new(),
            })
            .collect();
        let short = free
            .iter()
            .map(|hours| {
                let below = |h: u8| Bits::from_fn(hours.len(), |i| hours[i] < h);
                (1..=8).filter(|_| !hours.is_empty()).map(below).collect()
            })
            .collect();
        Self {
            today: table.today,
            held: vec![Vec::new(); free.len()],
            free,
            short,
        }
    }

    fn limited(&self, ask: &Ask) -> bool {
        ask.1 > 0 && !self.free[ask.0.0 as usize].is_empty()
    }

    fn change(&mut self, asks: &[Ask], days: &[Day], give_back: Option<&Table>) {
        let limited: Vec<Ask> = asks.iter().copied().filter(|a| self.limited(a)).collect();
        for ask in limited {
            let (row, hours) = (ask.0.0 as usize, ask.1);
            for day in days.iter().filter(|d| **d >= self.today) {
                let Some(free) = self.free[row].get_mut(day.index()) else {
                    continue;
                };
                *free = match give_back {
                    Some(table) => (*free + hours).min(table.hours(ask.0, *day)),
                    None => free.saturating_sub(hours),
                };
                for h in 1..=8u8 {
                    self.short[row][usize::from(h) - 1].set(*day, *free < h);
                }
            }
        }
    }

    /// Takes the hours asked on each of the days, for `arc`
    pub fn claim(&mut self, asks: &[Ask], days: &[Day], arc: ArcId) {
        self.change(asks, days, None);
        let (Some(first), Some(last)) = (days.first(), days.last()) else {
            return;
        };
        let limited: Vec<Ask> = asks.iter().copied().filter(|a| self.limited(a)).collect();
        for ask in limited {
            self.held[ask.0.0 as usize].push((*first, last.next(), arc));
        }
    }

    /// Gives back what `claim` took
    pub fn release(&mut self, table: &Table, asks: &[Ask], days: &[Day], arc: ArcId) {
        self.change(asks, days, Some(table));
        for ask in asks {
            self.held[ask.0.0 as usize].retain(|(_, _, holder)| *holder != arc);
        }
    }

    /// What holds hours of the row over a day
    pub fn holder(&self, row: RowId, day: Day) -> Option<ArcId> {
        let held = self.held[row.0 as usize].iter();
        let mut over = held.filter(|(start, end, _)| *start <= day && day < *end);
        over.next().map(|(_, _, arc)| *arc)
    }

    /// A valid day in `from..to` that other work leaves too few hours on, the last or the first
    fn blocked(
        &self,
        valid: &Valid,
        asks: &[Ask],
        from: Day,
        to: Day,
        last: bool,
    ) -> Option<(RowId, Day)> {
        let from = from.max(self.today);
        let found = asks.iter().filter(|a| self.limited(a)).filter_map(|ask| {
            let short = &self.short[ask.0.0 as usize][usize::from(ask.1.min(8)) - 1];
            let day = match last {
                true => short.last_common(valid.bits(), from, to),
                false => short.first_common(valid.bits(), from, to),
            };
            Some((ask.0, day?))
        });
        match last {
            true => found.max_by_key(|(row, day)| (*day, std::cmp::Reverse(*row))),
            false => found.min_by_key(|(row, day)| (*day, *row)),
        }
    }

    /// The first run of `n` valid days from `from` on that no other work breaks up, `None` if
    /// the table ends first. Days that are not valid may fall inside it, days other work holds
    /// may not.
    pub fn window(&self, valid: &Valid, asks: &[Ask], from: Day, n: u32) -> Option<Window> {
        let (mut from, mut blocker) = (from, None);
        if n == 0 {
            return Some(Window {
                start: from,
                end: from,
                blocker,
            });
        }
        loop {
            let start = valid.next(from)?;
            let end = valid.nth_from(start, n)?.next();
            match self.blocked(valid, asks, start, end, true) {
                Some((row, day)) => {
                    blocker = Some((row, day));
                    from = day.next();
                }
                None => {
                    return Some(Window {
                        start,
                        end,
                        blocker,
                    });
                }
            }
        }
    }

    /// The last such run that ends by `to` and starts no sooner than `floor`
    pub fn window_back(
        &self,
        valid: &Valid,
        asks: &[Ask],
        to: Day,
        n: u32,
        floor: Day,
    ) -> Option<(Day, Day)> {
        let mut to = to;
        if n == 0 {
            return (to >= floor).then_some((to, to));
        }
        loop {
            let end = valid.previous(to)?.next();
            let start = valid.nth_back(end, n)?;
            if start < floor {
                return None;
            }
            match self.blocked(valid, asks, start, end, false) {
                Some((_, day)) => to = day,
                None => return Some((start, end)),
            }
        }
    }

    /// Booked hours as a share of the hours the row has from its first to last booked day, 0 if
    /// nothing is booked
    pub fn load(&self, table: &Table, row: RowId) -> f32 {
        let (free, hours) = (&self.free[row.0 as usize], table.row_hours(row));
        let booked = |i: usize| u32::from(hours[i] - free[i]);
        let mut days = (0..free.len()).filter(|i| booked(*i) > 0);
        let (Some(first), Some(last)) = (days.clone().next(), days.next_back()) else {
            return 0.0;
        };
        let available: u32 = hours[first..=last].iter().map(|h| u32::from(*h)).sum();
        (first..=last).map(booked).sum::<u32>() as f32 / available as f32
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{availability::workload::Workload, scheduler_new::table::RowKey};
    use chrono::{Datelike, NaiveDate};
    use uuid::Uuid;

    /// One person on full weekdays over four weeks from Monday the 1st, today being that Monday
    fn setup() -> (Table, RowId, Ledger) {
        let origin = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut table = Table::new(origin, 28, origin);
        let user = table.insert(RowKey::User(Uuid::nil()), |d| {
            Workload::new_const(if d.weekday().number_from_monday() > 5 {
                0
            } else {
                8
            })
        });
        let ledger = Ledger::new(&table);
        (table, user, ledger)
    }

    fn days(days: &[u32]) -> Vec<Day> {
        days.iter().copied().map(Day).collect()
    }

    fn span(window: Option<Window>) -> Option<(u32, u32)> {
        window.map(|w| (w.start.0, w.end.0))
    }

    #[test]
    fn a_window_runs_across_weekends_but_starts_over_after_other_work() {
        let (table, user, mut ledger) = setup();
        let asks = [(user, 8)];
        let valid = table.valid(&asks, None, table.full());
        // Thursday, Friday and the Monday after
        assert_eq!(span(ledger.window(&valid, &asks, Day(3), 3)), Some((3, 8)));
        // half of Wednesday is held, which a full block cannot share
        ledger.claim(&[(user, 4)], &days(&[2]), ArcId(7));
        let window = ledger.window(&valid, &asks, Day(0), 3).unwrap();
        assert_eq!((window.start, window.end), (Day(3), Day(8)));
        assert_eq!(window.blocker, Some((user, Day(2))));
        assert_eq!(ledger.holder(user, Day(2)), Some(ArcId(7)));
        // a half block fits beside it
        let half = [(user, 4)];
        let valid = table.valid(&half, None, table.full());
        assert_eq!(span(ledger.window(&valid, &half, Day(0), 3)), Some((0, 3)));
        assert_eq!(span(ledger.window(&valid, &half, Day(0), 30)), None);
    }

    #[test]
    fn a_window_from_the_back_ends_as_late_as_it_can() {
        let (table, user, mut ledger) = setup();
        let asks = [(user, 8)];
        let valid = table.valid(&asks, None, table.full());
        // ending by the second Wednesday: Friday, Monday, Tuesday
        assert_eq!(
            ledger.window_back(&valid, &asks, Day(9), 3, Day(0)),
            Some((Day(4), Day(9)))
        );
        ledger.claim(&asks, &days(&[7]), ArcId(1));
        assert_eq!(
            ledger.window_back(&valid, &asks, Day(9), 3, Day(0)),
            Some((Day(2), Day(5)))
        );
        assert_eq!(ledger.window_back(&valid, &asks, Day(9), 3, Day(3)), None);
    }

    #[test]
    fn releases_give_hours_back_and_load_is_booked_over_available() {
        let (table, user, mut ledger) = setup();
        let asks = [(user, 8)];
        let valid = table.valid(&asks, None, table.full());
        assert_eq!(ledger.load(&table, user), 0.0);
        // Friday 4h and Monday 8h: 12 of the 16 hours across the two working days
        ledger.claim(&[(user, 4)], &days(&[4]), ArcId(1));
        ledger.claim(&asks, &days(&[7]), ArcId(2));
        assert_eq!(ledger.load(&table, user), 0.75);
        ledger.release(&table, &asks, &days(&[7]), ArcId(2));
        assert_eq!(ledger.holder(user, Day(7)), None);
        assert_eq!(span(ledger.window(&valid, &asks, Day(7), 1)), Some((7, 8)));
    }
}
// }}}
