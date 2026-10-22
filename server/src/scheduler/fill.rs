// modules {{{
use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::{
    availability::workload::Workload,
    planner::items::task::{Assignee, ScheduleMode},
};
// }}}

/// How many hours a person has on a day
pub trait Hours {
    /// Their own hours that day, from their calendar
    fn cap(&self, who: Assignee, day: NaiveDate) -> Workload;
    /// What nobody else has claimed yet, never more than `cap`
    fn free(&self, who: Assignee, day: NaiveDate) -> Workload;
}

// Hours supporting types {{{
/// A person working `hours` a day on a task
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub who: Assignee,
    pub hours: Workload,
}

/// What to fill and where, for slots that all work `duration` days (their effort is `hours *
/// duration`)
pub struct Spec<'a> {
    pub mode: ScheduleMode,
    pub duration: u32,
    pub start: NaiveDate,
    /// The first day not to look at
    pub stop: NaiveDate,
    /// Days before this are history: a restart never undoes them
    pub keep_before: NaiveDate,
    /// Keep working the daily block through this day even once the effort is done
    pub through: Option<NaiveDate>,
    /// Days the task may be worked, like an affinity
    pub allowed: &'a dyn Fn(NaiveDate) -> bool,
}

/// The hours placed for each slot, in slot order
#[derive(Debug, Default, PartialEq)]
pub struct Filled {
    pub days: Vec<BTreeMap<NaiveDate, Workload>>,
    /// Whether all the effort found days before `Spec::stop`
    pub complete: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Day {
    /// Someone cannot work a full block, which pauses the task without breaking a strict run
    Gap,
    /// Someone has the hours but another task holds them
    Blocked,
    Free,
}
// }}}

// impl Filled {{{
impl Filled {
    pub fn first(&self) -> Option<NaiveDate> {
        self.days
            .iter()
            .filter_map(|d| d.keys().next())
            .min()
            .copied()
    }

    pub fn last(&self) -> Option<NaiveDate> {
        let last = |d: &BTreeMap<NaiveDate, Workload>| d.keys().next_back().copied();
        self.days.iter().filter_map(last).max()
    }
}
// }}}

// impl Spec {{{
impl Spec<'_> {
    fn days(&self) -> impl Iterator<Item = NaiveDate> {
        let stop = self.stop;
        self.start.iter_days().take_while(move |d| *d < stop)
    }

    /// Whether the effort is done and nothing asks to keep working on `day`
    fn finished(&self, effort_done: bool, day: NaiveDate) -> bool {
        effort_done && self.through.is_none_or(|t| day > t)
    }
}
// }}}

// fill {{{
/// Places the slots from the start of the spec without changing anything. Strict tasks work whole
/// blocks on the same days for every slot, and a blocked day restarts the run, whereas relaxed
/// slots each take whatever is free.
pub fn fill(hours: &impl Hours, slots: &[Slot], spec: &Spec) -> Filled {
    match spec.mode {
        ScheduleMode::Strict => strict(hours, slots, spec),
        ScheduleMode::Relaxed => relaxed(hours, slots, spec),
    }
}
// }}}

// helpers {{{
fn day_state(hours: &impl Hours, slots: &[Slot], mode: ScheduleMode, day: NaiveDate) -> Day {
    let mut blocked = false;
    for slot in slots {
        if hours.cap(slot.who, day).get() < slot.hours.get() {
            return Day::Gap;
        }
        blocked |= mode.hours_on(slot.hours, hours.free(slot.who, day)) != slot.hours;
    }
    if blocked { Day::Blocked } else { Day::Free }
}

fn strict(hours: &impl Hours, slots: &[Slot], spec: &Spec) -> Filled {
    let duration = spec.duration as usize;
    let mut days: Vec<NaiveDate> = Vec::new();
    for day in spec.days() {
        if spec.finished(days.len() >= duration, day) {
            break;
        }
        if !(spec.allowed)(day) {
            continue;
        }
        match day_state(hours, slots, spec.mode, day) {
            Day::Gap => {}
            Day::Blocked if days.len() < duration => days.retain(|d| *d < spec.keep_before),
            Day::Blocked => {}
            Day::Free => days.push(day),
        }
    }
    Filled {
        complete: days.len() >= duration,
        days: slots
            .iter()
            .map(|s| days.iter().map(|d| (*d, s.hours)).collect())
            .collect(),
    }
}

fn relaxed(hours: &impl Hours, slots: &[Slot], spec: &Spec) -> Filled {
    let mut filled = Filled {
        days: Vec::new(),
        complete: true,
    };
    for slot in slots {
        let block = u32::from(slot.hours.get());
        let mut remaining = block * spec.duration;
        let mut taken = BTreeMap::new();
        for day in spec.days() {
            if spec.finished(remaining == 0, day) {
                break;
            }
            if !(spec.allowed)(day) {
                continue;
            }
            let wanted = if remaining == 0 {
                block
            } else {
                block.min(remaining)
            };
            let wanted = Workload::new(wanted as u8).expect("at most the block");
            let got = spec.mode.hours_on(wanted, hours.free(slot.who, day));
            if got.get() > 0 {
                taken.insert(day, got);
                remaining = remaining.saturating_sub(u32::from(got.get()));
            }
        }
        filled.complete &= remaining == 0;
        filled.days.push(taken);
    }
    filled
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn hours(h: u8) -> Workload {
        Workload::new_const(h)
    }

    /// Everyone works 8 hours on weekdays, January 2024 starts on a Monday. `taken` are hours
    /// other work holds on a day, which applies to every person.
    struct Mock {
        taken: Vec<(u32, u8)>,
    }

    impl Hours for Mock {
        fn cap(&self, _: Assignee, day: NaiveDate) -> Workload {
            let weekend = matches!(day.format("%a").to_string().as_str(), "Sat" | "Sun");
            hours(if weekend { 0 } else { 8 })
        }

        fn free(&self, who: Assignee, day: NaiveDate) -> Workload {
            let taken = self
                .taken
                .iter()
                .find(|(t, _)| d(*t) == day)
                .map_or(0, |t| t.1);
            hours(self.cap(who, day).get().saturating_sub(taken))
        }
    }

    fn person() -> Assignee {
        Assignee::User(Uuid::new_v4())
    }

    fn slot(who: Assignee, h: u8) -> Slot {
        Slot {
            who,
            hours: hours(h),
        }
    }

    fn spec(mode: ScheduleMode, duration: u32, allowed: &dyn Fn(NaiveDate) -> bool) -> Spec<'_> {
        Spec {
            mode,
            duration,
            start: d(1),
            stop: d(31),
            keep_before: d(1),
            through: None,
            allowed,
        }
    }

    fn days(filled: &Filled, slot: usize) -> Vec<u32> {
        use chrono::Datelike;
        filled.days[slot].keys().map(|d| d.day()).collect()
    }

    const ALWAYS: &dyn Fn(NaiveDate) -> bool = &|_| true;

    #[test]
    fn strict_runs_skip_weekends_without_restarting() {
        let mock = Mock { taken: vec![] };
        let who = person();
        let filled = fill(
            &mock,
            &[slot(who, 8)],
            &spec(ScheduleMode::Strict, 6, ALWAYS),
        );
        assert!(filled.complete);
        assert_eq!(days(&filled, 0), [1, 2, 3, 4, 5, 8]);
    }

    #[test]
    fn strict_runs_restart_on_a_blocked_working_day() {
        let mock = Mock {
            taken: vec![(3, 4)],
        };
        let who = person();
        let filled = fill(
            &mock,
            &[slot(who, 8)],
            &spec(ScheduleMode::Strict, 3, ALWAYS),
        );
        assert_eq!(days(&filled, 0), [4, 5, 8]);
        // a smaller block still fits beside the other work
        let filled = fill(
            &mock,
            &[slot(who, 4)],
            &spec(ScheduleMode::Strict, 3, ALWAYS),
        );
        assert_eq!(days(&filled, 0), [1, 2, 3]);
    }

    #[test]
    fn strict_slots_stay_in_step_and_restart_together() {
        let mock = Mock {
            taken: vec![(2, 2)],
        };
        let (a, b) = (person(), person());
        let slots = [slot(a, 8), slot(b, 4)];
        let filled = fill(&mock, &slots, &spec(ScheduleMode::Strict, 2, ALWAYS));
        assert_eq!(days(&filled, 0), [3, 4]);
        assert_eq!(days(&filled, 1), [3, 4]);
        assert_eq!(filled.days[1].values().map(|h| h.get()).sum::<u8>(), 8);
    }

    #[test]
    fn days_with_fewer_hours_than_the_block_are_gaps() {
        struct HalfFriday;
        impl Hours for HalfFriday {
            fn cap(&self, _: Assignee, day: NaiveDate) -> Workload {
                hours(match day.format("%a").to_string().as_str() {
                    "Sat" | "Sun" => 0,
                    "Fri" => 4,
                    _ => 8,
                })
            }
            fn free(&self, who: Assignee, day: NaiveDate) -> Workload {
                self.cap(who, day)
            }
        }
        let filled = fill(
            &HalfFriday,
            &[slot(person(), 8)],
            &spec(ScheduleMode::Strict, 5, ALWAYS),
        );
        assert_eq!(days(&filled, 0), [1, 2, 3, 4, 8]);
    }

    #[test]
    fn disallowed_days_pause_the_task() {
        let mock = Mock { taken: vec![] };
        let no_wednesdays = |day: NaiveDate| day != d(3);
        let filled = fill(
            &mock,
            &[slot(person(), 8)],
            &spec(ScheduleMode::Strict, 3, &no_wednesdays),
        );
        assert_eq!(days(&filled, 0), [1, 2, 4]);
    }

    #[test]
    fn relaxed_takes_whatever_is_free_until_the_effort_is_done() {
        let mock = Mock {
            taken: vec![(1, 5), (2, 8)],
        };
        let who = person();
        let filled = fill(
            &mock,
            &[slot(who, 8)],
            &spec(ScheduleMode::Relaxed, 2, ALWAYS),
        );
        // 3 hours on the 1st, none on the 2nd, then 8 and the last 5 on the 4th
        assert_eq!(days(&filled, 0), [1, 3, 4]);
        let hours: Vec<u8> = filled.days[0].values().map(|h| h.get()).collect();
        assert_eq!(hours, [3, 8, 5]);
        assert!(filled.complete);
    }

    #[test]
    fn running_out_of_days_is_incomplete() {
        let mock = Mock { taken: vec![] };
        let mut short = spec(ScheduleMode::Strict, 5, ALWAYS);
        short.stop = d(4);
        assert!(!fill(&mock, &[slot(person(), 8)], &short).complete);
        short.mode = ScheduleMode::Relaxed;
        assert!(!fill(&mock, &[slot(person(), 8)], &short).complete);
    }

    #[test]
    fn through_keeps_working_after_the_effort_and_history_survives_a_restart() {
        let mock = Mock {
            taken: vec![(4, 8)],
        };
        let who = person();
        let mut overrun = spec(ScheduleMode::Strict, 2, ALWAYS);
        overrun.through = Some(d(3));
        let filled = fill(&mock, &[slot(who, 8)], &overrun);
        assert_eq!(days(&filled, 0), [1, 2, 3]);

        // day 4 is held by other work, so the future part restarts but the past two days stay
        let mut restart = spec(ScheduleMode::Strict, 4, ALWAYS);
        restart.keep_before = d(3);
        let filled = fill(&mock, &[slot(who, 8)], &restart);
        assert_eq!(days(&filled, 0), [1, 2, 5, 8]);
    }
}
// }}}
