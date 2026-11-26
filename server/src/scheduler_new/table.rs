// modules {{{
use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::workload::Workload,
    common::short_id::ShortId,
    multitenancy::tenant::Tenant,
    planner::plan::Plan,
    scheduler_new::{
        bits::{Bits, Valid},
        day::Day,
    },
};
// }}}

/// Whose hours a row holds
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowKey {
    /// A person, the only rows work takes hours from
    User(Uuid),
    /// A plan's default availability, for its placeholder, its buffers and work nobody is on
    Plan(ShortId),
    /// Every day at full hours, for delays which count calendar days
    Full,
}

// RowKey supporting types {{{
/// A row in the table
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowId(pub u32);

/// The days a set of affinities all allow
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaskId(pub u32);

/// One row's worth of hours asked of it each day
pub type Ask = (RowId, u8);
// }}}

/// The hours of every person and plan default over the days of a run, and the days each
/// combination of task affinities allows, indexed by `Day`. Days past the end have no hours.
#[derive(Debug, Clone)]
pub struct Table {
    pub origin: NaiveDate,
    pub days: usize,
    /// The first day work can still take hours on, the days before it are history
    pub today: Day,
    keys: Vec<RowKey>,
    row_of: HashMap<RowKey, RowId>,
    hours: Vec<Vec<u8>>,
    /// For each row, the days it has at least 1 to 8 hours
    at_least: Vec<[Bits; 8]>,
    masks: Vec<Bits>,
    mask_of: HashMap<(ShortId, BTreeSet<Uuid>), MaskId>,
}

// impl Table {{{
impl Table {
    pub fn new(origin: NaiveDate, days: usize, today: NaiveDate) -> Self {
        let mut table = Self {
            origin,
            days,
            today: Day::of(today, origin),
            keys: Vec::new(),
            row_of: HashMap::new(),
            hours: Vec::new(),
            at_least: Vec::new(),
            masks: Vec::new(),
            mask_of: HashMap::new(),
        };
        table.insert(RowKey::Full, |_| Workload::MAX);
        table
    }

    /// Every tenant user and plan default in id order, from the calendars
    /// `Tenant::refresh_calendars` cached
    pub fn from_tenant(tenant: &Tenant, origin: NaiveDate, days: usize) -> Self {
        let mut table = Self::new(origin, days, tenant.today());
        let mut users: Vec<_> = tenant.users().keys().copied().collect();
        users.sort();
        for user in users {
            let calendar = tenant.calendar(&user);
            table.insert(RowKey::User(user), |d| {
                calendar.and_then(|c| c.on(d)).unwrap_or_default()
            });
        }
        let mut plans: Vec<_> = tenant.plans().keys().copied().collect();
        plans.sort();
        for plan in plans {
            let calendar = tenant.plan_calendar(&plan);
            table.insert(RowKey::Plan(plan), |d| {
                calendar.and_then(|c| c.on(d)).unwrap_or_default()
            });
        }
        table
    }

    pub fn insert(&mut self, key: RowKey, hours: impl Fn(NaiveDate) -> Workload) -> RowId {
        let row: Vec<u8> = (0..self.days)
            .map(|i| hours(Day(i as u32).date(self.origin)).get())
            .collect();
        let at_least =
            std::array::from_fn(|h| Bits::from_fn(self.days, |i| usize::from(row[i]) > h));
        let id = RowId(self.hours.len() as u32);
        self.row_of.insert(key, id);
        self.keys.push(key);
        self.hours.push(row);
        self.at_least.push(at_least);
        id
    }

    pub fn row(&self, key: &RowKey) -> Option<RowId> {
        self.row_of.get(key).copied()
    }

    pub fn full(&self) -> RowId {
        RowId(0)
    }

    pub fn key(&self, row: RowId) -> RowKey {
        self.keys[row.0 as usize]
    }

    pub fn rows(&self) -> usize {
        self.keys.len()
    }

    /// Whether work takes the row's hours, which only people run out of
    pub fn claimable(&self, row: RowId) -> bool {
        matches!(self.key(row), RowKey::User(_))
    }

    /// The row's hours on a day, none past the end
    pub fn hours(&self, row: RowId, day: Day) -> u8 {
        let row = &self.hours[row.0 as usize];
        row.get(day.index()).copied().unwrap_or(0)
    }

    pub fn row_hours(&self, row: RowId) -> &[u8] {
        &self.hours[row.0 as usize]
    }

    /// The days the row has at least `hours`, which is 1 to 8
    pub fn at_least(&self, row: RowId, hours: u8) -> &Bits {
        &self.at_least[row.0 as usize][usize::from(hours.clamp(1, 8)) - 1]
    }

    /// The mask for the days every affinity of a task allows, shared by tasks with the same set.
    /// `None` for a task with no affinities, which may be worked any day.
    pub fn mask(&mut self, plan: &Plan, affinities: BTreeSet<Uuid>) -> Option<MaskId> {
        if affinities.is_empty() {
            return None;
        }
        let key = (*plan.id(), affinities);
        if let Some(mask) = self.mask_of.get(&key) {
            return Some(*mask);
        }
        let last = Day(self.days.saturating_sub(1) as u32).date(self.origin);
        let mut bits = Bits::from_fn(self.days, |_| true);
        for affinity in key.1.iter().filter_map(|a| plan.affinities().get(a)) {
            let calendar = affinity.rules().calendar(self.origin, last);
            let allowed = calendar.values();
            bits = bits.and(&Bits::from_fn(self.days, |i| allowed[i]));
        }
        let id = MaskId(self.masks.len() as u32);
        self.masks.push(bits);
        self.mask_of.insert(key, id);
        Some(id)
    }

    /// The days work asking these hours of these rows can be done on: every row has its hours
    /// and the mask allows the day. Work that asks nothing takes the days `calendar` works.
    pub fn valid(&self, asks: &[Ask], mask: Option<MaskId>, calendar: RowId) -> Valid {
        let mut rows = asks.iter().map(|(row, hours)| self.at_least(*row, *hours));
        let mut bits = match rows.next() {
            Some(first) => rows.fold(first.clone(), |all, row| all.and(row)),
            None => self.at_least(calendar, 1).clone(),
        };
        if let Some(mask) = mask {
            bits = bits.and(&self.masks[mask.0 as usize]);
        }
        Valid::new(bits)
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    /// January 2024 starts on a Monday, the user works weekdays with a half day on Fridays
    fn table() -> (Table, RowId) {
        let origin = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut table = Table::new(origin, 14, origin);
        let user = table.insert(RowKey::User(Uuid::nil()), |d| {
            Workload::new_const(match d.weekday().number_from_monday() {
                6 | 7 => 0,
                5 => 4,
                _ => 8,
            })
        });
        (table, user)
    }

    #[test]
    fn rows_read_by_day_and_are_empty_past_the_end() {
        let (table, user) = table();
        assert_eq!(table.hours(user, Day(0)), 8);
        assert_eq!(table.hours(user, Day(5)), 0, "Saturday");
        assert_eq!(table.hours(user, Day(20)), 0, "past the end");
        assert_eq!(table.hours(table.full(), Day(5)), 8);
        assert!(table.claimable(user) && !table.claimable(table.full()));
    }

    #[test]
    fn valid_days_need_the_hours_asked_of_every_row() {
        let (table, user) = table();
        let full_days = table.valid(&[(user, 8)], None, table.full());
        // Fridays are half days, so a full block skips them like a weekend
        assert_eq!(full_days.nth_from(Day(3), 2), Some(Day(7)));
        let half_days = table.valid(&[(user, 4)], None, table.full());
        assert_eq!(half_days.nth_from(Day(3), 2), Some(Day(4)));
        // work that asks nothing counts the days of its calendar
        assert_eq!(table.valid(&[], None, table.full()).count(), 14);
        assert_eq!(table.valid(&[], None, user).count(), 10);
    }
}
// }}}
