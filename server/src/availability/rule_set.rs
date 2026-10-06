// modules{{{
use std::collections::HashMap;

use chrono::{NaiveDate, Weekday};
use uuid::Uuid;

use crate::{
    availability::{
        calendar::run_days,
        rule::{NewRule, Rule, RuleKind, RulePatch},
    },
    traits::entity::Entity,
    types::{
        crud::Crud,
        error::{PlinkoError, PlinkoResult},
        workload::Workload,
    },
};
// }}}

/// How much someone can work on each day, as a set of rules over a default weekly schedule
#[derive(Clone, Debug, PartialEq)]
pub struct RuleSet {
    pub(super) week_start: Weekday,
    pub(super) rules: HashMap<Uuid, Rule>,
}

pub enum RuleSetUpdate {
    Rule(Crud<NewRule, RulePatch>),
    WeekStart(Weekday),
}

// impl RuleSet{{{
impl Default for RuleSet {
    fn default() -> Self {
        Self {
            week_start: Weekday::Mon,
            rules: HashMap::new(),
        }
    }
}

impl RuleSet {
    pub fn week_start(&self) -> Weekday {
        self.week_start
    }

    pub fn rules(&self) -> &HashMap<Uuid, Rule> {
        &self.rules
    }

    pub fn apply_update(&mut self, update: RuleSetUpdate) -> PlinkoResult<()> {
        match update {
            RuleSetUpdate::Rule(update) => Rule::apply_crud(self, update),
            RuleSetUpdate::WeekStart(weekday) => {
                self.week_start = weekday;
                Ok(())
            }
        }
    }

    /// Hours available on `day`. Use `calendar` to read many days.
    pub fn availability_on(&self, day: NaiveDate) -> Workload {
        self.calendar(day, day)
            .on(day)
            .expect("calendar covers the day")
    }

    /// Every rule in effect on `day`, whether or not it changes that day, lowest priority first.
    /// A finite run of workdays is in effect from its start until its last day.
    pub fn active_rules(&self, day: NaiveDate) -> Vec<&Rule> {
        let driving = self.driving_calendar(self.lookback(day), day);
        let mut rules: Vec<_> = self
            .in_effect(day)
            .filter(|r| match r.kind() {
                RuleKind::FiniteWorkdays { days, .. } => {
                    let run = run_days(r, &driving, day);
                    run.len() < *days as usize || run.last() == Some(&day)
                }
                _ => true,
            })
            .collect();
        rules.sort_by_key(|r| (rank(r.kind()), r.start_day(), *r.id()));
        rules
    }

    fn in_effect(&self, day: NaiveDate) -> impl Iterator<Item = &Rule> {
        self.rules.values().filter(move |r| r.is_in_effect(day))
    }
}

fn rank(kind: &RuleKind) -> u8 {
    match kind {
        RuleKind::Weekly(_) => 0,
        RuleKind::Monthly(_) => 1,
        RuleKind::FiniteWorkdays { .. } => 2,
        RuleKind::Holiday => 3,
    }
}
// }}}

impl Entity<RuleSet> for Rule {
    type New = NewRule;
    type Patch = RulePatch;

    fn entities(set: &RuleSet) -> &HashMap<Uuid, Self> {
        &set.rules
    }
    fn entities_mut(set: &mut RuleSet) -> &mut HashMap<Uuid, Self> {
        &mut set.rules
    }

    fn validate(set: &RuleSet, id: Uuid) -> PlinkoResult<()> {
        let rule = &set.rules[&id];
        let stops_before_start = rule.stop_day().is_some_and(|s| s <= rule.start_day());
        let empty_run = matches!(rule.kind(), RuleKind::FiniteWorkdays { days: 0, .. });
        match stops_before_start || empty_run {
            true => Err(PlinkoError::InvalidRule(id)),
            false => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Weekday::{Mon, Sun, Wed};

    use super::*;
    use crate::availability::rule::{MonthTarget, MonthlyOverride, Ordinal, WeeklySchedule};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn hours(set: &RuleSet, y: i32, m: u32, day: u32) -> u8 {
        set.availability_on(d(y, m, day)).get()
    }

    fn add(set: &mut RuleSet, start_day: NaiveDate, kind: RuleKind) -> Uuid {
        let id = Uuid::now_v7();
        let new = NewRule {
            id,
            start_day,
            kind,
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .unwrap();
        id
    }

    fn stop(set: &mut RuleSet, id: Uuid, day: NaiveDate) -> PlinkoResult<()> {
        let patch = RulePatch {
            id,
            start_day: None,
            stop_day: Some(Some(day)),
            kind: None,
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Update(patch)))
    }

    fn monthly(ordinal: Ordinal, target: MonthTarget, hours: u8, priority: u32) -> RuleKind {
        RuleKind::Monthly(MonthlyOverride {
            ordinal,
            target,
            hours: Workload::new(hours).unwrap(),
            priority,
        })
    }

    // January 2024 starts on a Monday and has five of them, February 2024 has four
    #[test]
    fn defaults_to_eight_hour_weekdays() {
        let set = RuleSet::default();
        assert_eq!(hours(&set, 2024, 1, 1), 8);
        assert_eq!(hours(&set, 2024, 1, 6), 0);
    }

    #[test]
    fn weekly_rule_applies_from_its_start_until_stopped() {
        let mut set = RuleSet::default();
        let part_time = WeeklySchedule::new([4, 4, 4, 4, 4, 0, 0]).unwrap();
        let id = add(&mut set, d(2024, 1, 8), RuleKind::Weekly(part_time));
        stop(&mut set, id, d(2024, 1, 15)).unwrap();
        assert_eq!(hours(&set, 2024, 1, 5), 8);
        assert_eq!(hours(&set, 2024, 1, 9), 4);
        assert_eq!(hours(&set, 2024, 1, 15), 8);
    }

    #[test]
    fn missing_ordinals_fall_back_to_the_nearest_valid() {
        let mut set = RuleSet::default();
        let fifth_monday = monthly(Ordinal::Fifth, MonthTarget::Weekday(Mon), 2, 0);
        add(&mut set, d(2024, 1, 1), fifth_monday);
        assert_eq!(hours(&set, 2024, 1, 29), 2);
        assert_eq!(hours(&set, 2024, 2, 26), 2);
        assert_eq!(hours(&set, 2024, 2, 19), 8);
    }

    #[test]
    fn day_targets_count_from_either_end() {
        let mut set = RuleSet::default();
        add(
            &mut set,
            d(2024, 1, 1),
            monthly(Ordinal::Last, MonthTarget::Day, 1, 0),
        );
        let second_last = monthly(Ordinal::SecondLast, MonthTarget::Day, 3, 0);
        add(&mut set, d(2024, 1, 1), second_last);
        assert_eq!(hours(&set, 2024, 1, 31), 1);
        assert_eq!(hours(&set, 2024, 1, 30), 3);
        assert_eq!(hours(&set, 2024, 2, 29), 1);
    }

    #[test]
    fn week_targets_use_full_weeks_from_the_week_start() {
        let mut set = RuleSet::default();
        let toggles = [true, false, false, false, false, false, false];
        let first_week = monthly(Ordinal::First, MonthTarget::Week(toggles), 2, 0);
        add(&mut set, d(2024, 1, 1), first_week);
        assert_eq!(hours(&set, 2024, 1, 1), 2);
        assert_eq!(hours(&set, 2024, 1, 8), 8);

        // with Sunday weeks January's first full week is the 7th to the 13th
        set.apply_update(RuleSetUpdate::WeekStart(Sun)).unwrap();
        assert_eq!(hours(&set, 2024, 1, 1), 8);
        assert_eq!(hours(&set, 2024, 1, 8), 2);
    }

    #[test]
    fn higher_priority_then_later_start_wins_between_monthly_rules() {
        let mut set = RuleSet::default();
        let first_day = monthly(Ordinal::First, MonthTarget::Day, 2, 1);
        let first_monday = monthly(Ordinal::First, MonthTarget::Weekday(Mon), 5, 2);
        add(&mut set, d(2024, 1, 1), first_day);
        add(&mut set, d(2024, 1, 1), first_monday);
        assert_eq!(hours(&set, 2024, 1, 1), 5);

        let later = monthly(Ordinal::First, MonthTarget::Day, 7, 2);
        add(&mut set, d(2024, 1, 2), later);
        assert_eq!(hours(&set, 2024, 1, 1), 5);
        assert_eq!(hours(&set, 2024, 2, 1), 7);
    }

    #[test]
    fn finite_workdays_skip_days_that_are_not_working() {
        let mut set = RuleSet::default();
        let run = RuleKind::FiniteWorkdays {
            days: 3,
            hours: Workload::new(2).unwrap(),
        };
        let id = add(&mut set, d(2024, 1, 4), run);
        let results: Vec<_> = (4..=9).map(|day| hours(&set, 2024, 1, day)).collect();
        assert_eq!(results, [2, 2, 0, 0, 2, 8]);

        assert!(
            set.active_rules(d(2024, 1, 8))
                .iter()
                .any(|r| *r.id() == id)
        );
        assert!(set.active_rules(d(2024, 1, 9)).is_empty());
    }

    #[test]
    fn workdays_follow_the_weekly_and_monthly_rules_for_what_counts() {
        let mut set = RuleSet::default();
        add(
            &mut set,
            d(2023, 12, 1),
            monthly(Ordinal::First, MonthTarget::Day, 0, 0),
        );
        let run = RuleKind::FiniteWorkdays {
            days: 3,
            hours: Workload::new(1).unwrap(),
        };
        add(&mut set, d(2023, 12, 29), run);
        // Fri the 29th counts, the weekend and Monday the 1st (0h) do not, then Tue and Wed
        let results: Vec<_> = [(12, 29), (12, 30), (1, 1), (1, 2), (1, 3), (1, 4)]
            .iter()
            .map(|&(m, day)| hours(&set, if m == 12 { 2023 } else { 2024 }, m, day))
            .collect();
        assert_eq!(results, [1, 0, 0, 1, 1, 8]);
    }

    #[test]
    fn calendar_matches_day_by_day_answers() {
        let mut set = RuleSet::default();
        let part_time = WeeklySchedule::new([4, 4, 4, 4, 4, 0, 0]).unwrap();
        add(&mut set, d(2024, 1, 15), RuleKind::Weekly(part_time));
        let second_wed = monthly(Ordinal::Second, MonthTarget::Weekday(Wed), 1, 3);
        add(&mut set, d(2024, 1, 1), second_wed);
        let run = RuleKind::FiniteWorkdays {
            days: 9,
            hours: Workload::new(2).unwrap(),
        };
        add(&mut set, d(2024, 1, 10), run);
        let holiday = add(&mut set, d(2024, 1, 22), RuleKind::Holiday);
        stop(&mut set, holiday, d(2024, 1, 24)).unwrap();

        // the range starts inside the workday run and runs over month ends
        let (from, to) = (d(2024, 1, 12), d(2024, 3, 10));
        let calendar = set.calendar(from, to);
        assert_eq!((calendar.start(), calendar.end()), (from, to));
        for day in from.iter_days().take_while(|day| *day <= to) {
            let expected = set.availability_on(day);
            assert_eq!(calendar.on(day), Some(expected), "{day}");
        }
        assert_eq!(calendar.on(d(2024, 1, 11)), None);
        assert!(set.calendar(to, from).hours().is_empty());
    }

    #[test]
    fn holidays_win_until_stopped() {
        let mut set = RuleSet::default();
        let run = RuleKind::FiniteWorkdays {
            days: 10,
            hours: Workload::new(4).unwrap(),
        };
        add(&mut set, d(2024, 1, 1), run);
        let holiday = add(&mut set, d(2024, 1, 8), RuleKind::Holiday);
        stop(&mut set, holiday, d(2024, 1, 10)).unwrap();
        assert_eq!(hours(&set, 2024, 1, 5), 4);
        assert_eq!(hours(&set, 2024, 1, 8), 0);
        assert_eq!(hours(&set, 2024, 1, 9), 0);
        assert_eq!(hours(&set, 2024, 1, 10), 4);
    }

    #[test]
    fn active_rules_include_rules_that_do_not_change_the_day() {
        let mut set = RuleSet::default();
        let first_monday = monthly(Ordinal::First, MonthTarget::Weekday(Mon), 2, 0);
        let monthly_id = add(&mut set, d(2024, 1, 1), first_monday);
        let holiday_id = add(&mut set, d(2024, 3, 1), RuleKind::Holiday);
        let ids = |day| -> Vec<Uuid> { set.active_rules(day).iter().map(|r| *r.id()).collect() };
        assert_eq!(ids(d(2024, 1, 2)), [monthly_id]);
        assert_eq!(ids(d(2024, 3, 2)), [monthly_id, holiday_id]);
        assert!(ids(d(2023, 12, 31)).is_empty());
    }

    #[test]
    fn invalid_rules_are_rejected_and_reverted() {
        let mut set = RuleSet::default();
        let id = add(&mut set, d(2024, 1, 8), RuleKind::Holiday);
        assert!(stop(&mut set, id, d(2024, 1, 8)).is_err());
        assert_eq!(set.rules()[&id].stop_day(), None);

        let empty = RuleKind::FiniteWorkdays {
            days: 0,
            hours: Workload::new(2).unwrap(),
        };
        let new = NewRule {
            id: Uuid::now_v7(),
            start_day: d(2024, 1, 1),
            kind: empty,
        };
        assert!(
            set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
                .is_err()
        );
        assert_eq!(set.rules().len(), 1);
    }
}
