// modules{{{
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::{
        calendar::rank_of,
        day_value::DayValue,
        rule::{
            DEFAULT_RULE, EPOCH, MAX_NAME, NewRule, RULE_START_RANGE, Rule, RuleKind, RulePatch,
        },
    },
    common::{
        crud::Crud,
        error::{PlinkoError, PlinkoResult},
    },
    traits::entity::Entity,
};
// }}}

/// How much someone can work on each day, as a set of rules over a default weekly schedule
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleSet<V> {
    pub(super) rules: HashMap<Uuid, Rule<V>>,
}

// RuleSet supporting types {{{
#[derive(Clone, Serialize, Deserialize)]
pub enum RuleSetUpdate<V> {
    Rule(Crud<NewRule<V>, RulePatch<V>>),
}

impl<V> RuleSetUpdate<V> {
    /// Gives any new rule a fresh id, see `Crud::assign_id`
    pub fn assign_ids(&mut self) {
        match self {
            RuleSetUpdate::Rule(update) => update.assign_id(),
        }
    }
}
// }}}

// impl RuleSet{{{
impl<V: DayValue> Default for RuleSet<V> {
    fn default() -> Self {
        let rule = Rule::default_weekly();
        Self {
            rules: HashMap::from([(*rule.id(), rule)]),
        }
    }
}

impl<V: DayValue> RuleSet<V> {
    pub fn rules(&self) -> &HashMap<Uuid, Rule<V>> {
        &self.rules
    }

    pub fn apply_update(&mut self, update: RuleSetUpdate<V>) -> PlinkoResult<()> {
        match update {
            RuleSetUpdate::Rule(update) => Rule::apply_crud(self, update),
        }
    }

    /// The value on `day`. Use `calendar` to read many days.
    pub fn value_on(&self, day: NaiveDate) -> V {
        self.calendar(day, day)
            .on(day)
            .expect("calendar covers the day")
    }

    /// Every rule with a say in `day`, lowest priority first
    pub fn active_rules(&self, day: NaiveDate) -> Vec<&Rule<V>> {
        let mut rules: Vec<_> = self.rules.values().filter(|r| r.applies_on(day)).collect();
        rules.sort_by_key(|r| (rank_of(r.kind()), r.start_day(), *r.id()));
        rules
    }
}
// }}}

// impl Entity {{{
impl<V: DayValue> Entity<RuleSet<V>> for Rule<V> {
    type New = NewRule<V>;
    type Patch = RulePatch<V>;

    fn entities(set: &RuleSet<V>) -> &HashMap<Uuid, Self> {
        &set.rules
    }
    fn entities_mut(set: &mut RuleSet<V>) -> &mut HashMap<Uuid, Self> {
        &mut set.rules
    }

    fn validate(set: &RuleSet<V>, id: Uuid) -> PlinkoResult<()> {
        let rule = &set.rules[&id];
        let invalid = if id == DEFAULT_RULE {
            // the default stays a weekly schedule from the epoch, whose days only can change
            rule.start_day() != EPOCH
                || rule.stop_day().is_some()
                || !matches!(rule.kind(), RuleKind::Weekly(_))
        } else {
            let stops_before_start = rule.stop_day().is_some_and(|s| s <= rule.start_day());
            let out_of_range = !RULE_START_RANGE.contains(&rule.start_day());
            let bad_kind = match rule.kind() {
                RuleKind::Holiday => rule.stop_day().is_none(),
                RuleKind::Override(o) => o.days == 0 || o.every.is_some_and(|e| e.n == 0),
                RuleKind::Weekly(_) | RuleKind::ResetWeekly => false,
            };
            // only one change of weekly schedule starts on a day
            let clashes = is_weekly(rule.kind())
                && set.rules.values().any(|other| {
                    other.id() != rule.id()
                        && *other.id() != DEFAULT_RULE
                        && is_weekly(other.kind())
                        && other.start_day() == rule.start_day()
                });
            let name_too_long = rule.name().chars().count() > MAX_NAME;
            stops_before_start || out_of_range || bad_kind || clashes || name_too_long
        };
        match invalid {
            true => Err(PlinkoError::InvalidRule(id)),
            false => Ok(()),
        }
    }

    fn in_use(_set: &RuleSet<V>, id: Uuid) -> bool {
        id == DEFAULT_RULE
    }
}
// }}}

// is_weekly {{{
fn is_weekly<V>(kind: &RuleKind<V>) -> bool {
    matches!(kind, RuleKind::Weekly(_) | RuleKind::ResetWeekly)
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        availability::rule::{DayOverride, Every, Unit, WeeklySchedule},
        availability::workload::Workload,
    };

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn hours(set: &RuleSet<Workload>, y: i32, m: u32, day: u32) -> u8 {
        set.value_on(d(y, m, day)).get()
    }

    fn add(
        set: &mut RuleSet<Workload>,
        start_day: NaiveDate,
        stop_day: Option<NaiveDate>,
        kind: RuleKind<Workload>,
    ) -> PlinkoResult<Uuid> {
        let id = Uuid::now_v7();
        let new = NewRule {
            name: String::new(),
            stop_day,
            id,
            start_day,
            kind,
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .map(|()| id)
    }

    fn over(days: u32, hours: u8, every: Option<(u32, Unit)>) -> RuleKind<Workload> {
        RuleKind::Override(DayOverride {
            days,
            value: Workload::new(hours).unwrap(),
            every: every.map(|(n, unit)| Every {
                n,
                unit,
                from_end: false,
            }),
        })
    }

    // January 2024 starts on a Monday
    #[test]
    fn a_set_starts_with_eight_hour_weekdays_from_the_epoch() {
        let set = RuleSet::<Workload>::default();
        assert_eq!(hours(&set, 2024, 1, 1), 8);
        assert_eq!(hours(&set, 2024, 1, 6), 0);
        assert_eq!(set.rules()[&DEFAULT_RULE].start_day(), EPOCH);
        assert_eq!(set.value_on(d(1971, 1, 4)).get(), 8);
    }

    #[test]
    fn the_default_cannot_be_removed_moved_or_made_something_else() {
        let mut set = RuleSet::<Workload>::default();
        let remove = RuleSetUpdate::Rule(Crud::Remove(DEFAULT_RULE));
        assert!(set.apply_update(remove).is_err());
        let patch = |start_day, kind| {
            RuleSetUpdate::Rule(Crud::Update(RulePatch {
                id: DEFAULT_RULE,
                start_day,
                stop_day: None,
                kind,
                name: None,
            }))
        };
        assert!(set.apply_update(patch(Some(d(2024, 1, 1)), None)).is_err());
        assert!(
            set.apply_update(patch(None, Some(RuleKind::Holiday)))
                .is_err()
        );
        let part_time = WeeklySchedule::from_hours([4, 4, 4, 4, 4, 0, 0]).unwrap();
        set.apply_update(patch(None, Some(RuleKind::Weekly(part_time))))
            .unwrap();
        assert_eq!(hours(&set, 2024, 1, 1), 4);
    }

    #[test]
    fn weekly_rule_applies_from_its_start_until_stopped() {
        let mut set = RuleSet::<Workload>::default();
        let part_time = WeeklySchedule::from_hours([4, 4, 4, 4, 4, 0, 0]).unwrap();
        add(
            &mut set,
            d(2024, 1, 8),
            Some(d(2024, 1, 15)),
            RuleKind::Weekly(part_time),
        )
        .unwrap();
        assert_eq!(hours(&set, 2024, 1, 5), 8);
        assert_eq!(hours(&set, 2024, 1, 9), 4);
        assert_eq!(hours(&set, 2024, 1, 15), 8);
    }

    #[test]
    fn an_override_sets_its_days_once() {
        let mut set = RuleSet::<Workload>::default();
        add(&mut set, d(2024, 1, 9), None, over(3, 2, None)).unwrap();
        let results: Vec<_> = (8..=13).map(|day| hours(&set, 2024, 1, day)).collect();
        // Tuesday to Thursday are 2 hours, and it does not come round again
        assert_eq!(results, [8, 2, 2, 2, 8, 0]);
        assert_eq!(hours(&set, 2024, 1, 16), 8);
    }

    #[test]
    fn overrides_repeat_by_days_weeks_months_and_years_until_stopped() {
        let mut set = RuleSet::<Workload>::default();
        // every second Monday from the 1st, two days at a time, until the end of February
        add(
            &mut set,
            d(2024, 1, 1),
            Some(d(2024, 3, 1)),
            over(2, 1, Some((2, Unit::Week))),
        )
        .unwrap();
        assert_eq!(hours(&set, 2024, 1, 1), 1);
        assert_eq!(hours(&set, 2024, 1, 2), 1);
        assert_eq!(hours(&set, 2024, 1, 3), 8);
        assert_eq!(hours(&set, 2024, 1, 8), 8);
        assert_eq!(hours(&set, 2024, 1, 15), 1);
        assert_eq!(hours(&set, 2024, 2, 26), 1);
        assert_eq!(hours(&set, 2024, 3, 11), 8);

        let mut set = RuleSet::<Workload>::default();
        // the 31st each month shortens in months that end sooner
        add(
            &mut set,
            d(2024, 1, 31),
            None,
            over(1, 3, Some((1, Unit::Month))),
        )
        .unwrap();
        assert_eq!(hours(&set, 2024, 2, 29), 3);
        assert_eq!(hours(&set, 2024, 3, 31), 3);
        assert_eq!(hours(&set, 2024, 3, 29), 8);

        let mut set = RuleSet::<Workload>::default();
        add(
            &mut set,
            d(2024, 2, 29),
            None,
            over(1, 0, Some((1, Unit::Year))),
        )
        .unwrap();
        assert_eq!(hours(&set, 2025, 2, 28), 0);
        assert_eq!(hours(&set, 2028, 2, 29), 0);
        assert_eq!(hours(&set, 2025, 2, 27), 8);

        let mut set = RuleSet::<Workload>::default();
        add(
            &mut set,
            d(2024, 1, 1),
            None,
            over(1, 2, Some((3, Unit::Day))),
        )
        .unwrap();
        let results: Vec<_> = (1..=7).map(|day| hours(&set, 2024, 1, day)).collect();
        assert_eq!(results, [2, 8, 8, 2, 8, 0, 2]);
    }

    fn aligned(start: NaiveDate, from_end: bool) -> RuleSet<Workload> {
        let mut set = RuleSet::<Workload>::default();
        let every = Every {
            n: 1,
            unit: Unit::MonthAligned,
            from_end,
        };
        let kind = RuleKind::Override(DayOverride {
            days: 1,
            value: Workload::new(1).unwrap(),
            every: Some(every),
        });
        add(&mut set, start, None, kind).unwrap();
        set
    }

    #[test]
    fn aligned_months_follow_the_nth_weekday_of_the_month() {
        // the second Monday of January 2024 is the 8th
        let set = aligned(d(2024, 1, 8), false);
        assert_eq!(hours(&set, 2024, 2, 12), 1);
        assert_eq!(hours(&set, 2024, 3, 11), 1);
        assert_eq!(hours(&set, 2024, 4, 8), 1);
        // not the same day number, and not the other Mondays
        assert_eq!(hours(&set, 2024, 2, 8), 8);
        assert_eq!(hours(&set, 2024, 2, 5), 8);
        assert_eq!(hours(&set, 2024, 2, 19), 8);
    }

    #[test]
    fn aligned_months_can_count_weekdays_from_the_end() {
        // the last Tuesday of January 2024 is the 30th
        let set = aligned(d(2024, 1, 30), true);
        assert_eq!(hours(&set, 2024, 2, 27), 1);
        assert_eq!(hours(&set, 2024, 3, 26), 1);
        assert_eq!(hours(&set, 2024, 2, 20), 8);
        // counting from the start the same day is the fifth, which February does not have
        let set = aligned(d(2024, 1, 30), false);
        assert_eq!(hours(&set, 2024, 2, 27), 1);
        assert_eq!(hours(&set, 2024, 2, 6), 8);
    }

    #[test]
    fn a_month_without_the_weekday_that_many_times_uses_its_last() {
        // the fifth Monday of January 2024 is the 29th, February and March have only four
        let set = aligned(d(2024, 1, 29), false);
        assert_eq!(hours(&set, 2024, 2, 26), 1);
        assert_eq!(hours(&set, 2024, 3, 25), 1);
        // April 2024 starts on a Monday so has five
        assert_eq!(hours(&set, 2024, 4, 29), 1);
        assert_eq!(hours(&set, 2024, 4, 22), 8);
    }

    #[test]
    fn later_rules_of_a_kind_win_and_holidays_win_over_overrides() {
        let mut set = RuleSet::<Workload>::default();
        add(&mut set, d(2024, 1, 8), None, over(5, 4, None)).unwrap();
        add(&mut set, d(2024, 1, 10), None, over(1, 6, None)).unwrap();
        assert_eq!(hours(&set, 2024, 1, 9), 4);
        assert_eq!(hours(&set, 2024, 1, 10), 6);
        add(
            &mut set,
            d(2024, 1, 9),
            Some(d(2024, 1, 11)),
            RuleKind::Holiday,
        )
        .unwrap();
        assert_eq!(hours(&set, 2024, 1, 9), 0);
        assert_eq!(hours(&set, 2024, 1, 10), 0);
        assert_eq!(hours(&set, 2024, 1, 11), 4);
    }

    #[test]
    fn calendar_matches_day_by_day_answers() {
        let mut set = RuleSet::<Workload>::default();
        let part_time = WeeklySchedule::from_hours([4, 4, 4, 4, 4, 0, 0]).unwrap();
        add(&mut set, d(2024, 1, 15), None, RuleKind::Weekly(part_time)).unwrap();
        add(
            &mut set,
            d(2024, 1, 10),
            None,
            over(2, 1, Some((10, Unit::Day))),
        )
        .unwrap();
        add(
            &mut set,
            d(2024, 1, 22),
            Some(d(2024, 1, 24)),
            RuleKind::Holiday,
        )
        .unwrap();
        let (from, to) = (d(2024, 1, 12), d(2024, 3, 10));
        let calendar = set.calendar(from, to);
        assert_eq!((calendar.start(), calendar.end()), (from, to));
        for day in from.iter_days().take_while(|day| *day <= to) {
            let expected = set.value_on(day);
            assert_eq!(calendar.on(day), Some(expected), "{day}");
        }
        assert_eq!(calendar.on(d(2024, 1, 11)), None);
        assert!(set.calendar(to, from).values().is_empty());
    }

    #[test]
    fn active_rules_are_the_ones_with_a_say_in_the_day() {
        let mut set = RuleSet::<Workload>::default();
        let one = add(&mut set, d(2024, 1, 9), None, over(1, 2, None)).unwrap();
        let holiday = add(
            &mut set,
            d(2024, 1, 9),
            Some(d(2024, 1, 12)),
            RuleKind::Holiday,
        )
        .unwrap();
        let ids = |day| -> Vec<Uuid> { set.active_rules(day).iter().map(|r| *r.id()).collect() };
        assert_eq!(ids(d(2024, 1, 8)), [DEFAULT_RULE]);
        assert_eq!(ids(d(2024, 1, 9)), [DEFAULT_RULE, one, holiday]);
        assert_eq!(ids(d(2024, 1, 10)), [DEFAULT_RULE, holiday]);
        assert_eq!(ids(d(2024, 1, 12)), [DEFAULT_RULE]);
    }

    #[test]
    fn rules_must_start_within_the_supported_years() {
        let mut set = RuleSet::<Workload>::default();
        let holiday = |set: &mut RuleSet<Workload>, start_day: NaiveDate| {
            add(
                set,
                start_day,
                Some(start_day + chrono::Days::new(1)),
                RuleKind::Holiday,
            )
        };
        assert!(holiday(&mut set, d(1999, 12, 31)).is_err());
        assert!(holiday(&mut set, d(2101, 1, 1)).is_err());
        assert!(
            add(
                &mut set,
                NaiveDate::MIN,
                Some(d(2024, 1, 1)),
                RuleKind::Holiday
            )
            .is_err()
        );
        holiday(&mut set, d(2000, 1, 1)).unwrap();
        assert_eq!(set.rules().len(), 2);
    }

    #[test]
    fn invalid_rules_are_rejected_and_reverted() {
        let mut set = RuleSet::<Workload>::default();
        // a holiday has to end, and nothing stops before it starts or is empty
        assert!(add(&mut set, d(2024, 1, 8), None, RuleKind::Holiday).is_err());
        let id = add(
            &mut set,
            d(2024, 1, 8),
            Some(d(2024, 1, 9)),
            RuleKind::Holiday,
        )
        .unwrap();
        let stop = RulePatch {
            id,
            start_day: None,
            stop_day: Some(Some(d(2024, 1, 8))),
            kind: None,
            name: None,
        };
        assert!(
            set.apply_update(RuleSetUpdate::Rule(Crud::Update(stop)))
                .is_err()
        );
        assert_eq!(set.rules()[&id].stop_day(), Some(d(2024, 1, 9)));
        assert!(add(&mut set, d(2024, 1, 8), None, over(0, 2, None)).is_err());
        assert!(
            add(
                &mut set,
                d(2024, 1, 8),
                None,
                over(1, 2, Some((0, Unit::Day)))
            )
            .is_err()
        );
        assert_eq!(set.rules().len(), 2);
    }

    #[test]
    fn resetting_goes_back_to_the_default_week_and_one_weekly_change_starts_a_day() {
        let mut set = RuleSet::<Workload>::default();
        let part_time = WeeklySchedule::from_hours([4, 4, 4, 4, 4, 0, 0]).unwrap();
        add(
            &mut set,
            d(2024, 1, 8),
            None,
            RuleKind::Weekly(part_time.clone()),
        )
        .unwrap();
        add(&mut set, d(2024, 1, 22), None, RuleKind::ResetWeekly).unwrap();
        assert_eq!(hours(&set, 2024, 1, 15), 4);
        assert_eq!(hours(&set, 2024, 1, 22), 8);
        assert_eq!(
            set.calendar(d(2024, 1, 15), d(2024, 1, 23))
                .on(d(2024, 1, 23)),
            Some(Workload::new(8).unwrap())
        );
        // not on a day that already has a weekly change, whichever comes second
        assert!(add(&mut set, d(2024, 1, 8), None, RuleKind::ResetWeekly).is_err());
        assert!(add(&mut set, d(2024, 1, 22), None, RuleKind::Weekly(part_time)).is_err());
        // but other rules may share the day
        add(&mut set, d(2024, 1, 22), None, over(1, 2, None)).unwrap();
    }

    #[test]
    fn names_are_kept_and_limited() {
        let mut set = RuleSet::<Workload>::default();
        let mut new = |name: String| {
            let new = NewRule {
                name,
                stop_day: None,
                id: Uuid::now_v7(),
                start_day: d(2024, 1, 8),
                kind: over(1, 2, None),
            };
            set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
        };
        assert!(new("a".repeat(MAX_NAME)).is_ok());
        assert!(new("a".repeat(MAX_NAME + 1)).is_err());
        assert!(new("é".repeat(MAX_NAME)).is_ok());
    }

    #[test]
    fn boolean_rule_sets_default_to_every_day_and_use_the_same_rules() {
        let mut set = RuleSet::<bool>::default();
        let allowed = |set: &RuleSet<bool>, day| set.value_on(d(2024, 1, day));
        assert!((1..=7).all(|day| allowed(&set, day)));
        let new = NewRule {
            name: String::new(),
            stop_day: None,
            id: Uuid::now_v7(),
            start_day: d(2024, 1, 3),
            kind: RuleKind::Override(DayOverride {
                days: 2,
                value: false,
                every: None,
            }),
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .unwrap();
        let results: Vec<_> = (2..=5).map(|day| allowed(&set, day)).collect();
        assert_eq!(results, [true, false, false, true]);
    }
}
// }}}
