use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSchedule {
    pub days: HashMap<Weekday, f32>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl WorkSchedule {
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

    pub fn full_week() -> Self {
        let mut s = Self::weekdays();
        s.days.insert(Weekday::Saturday, 8.0);
        s.days.insert(Weekday::Sunday, 8.0);
        s
    }

    pub fn with_day(mut self, day: Weekday, hours: f32) -> Self {
        self.days.insert(day, hours);
        self
    }

    pub fn without_day(mut self, day: Weekday) -> Self {
        self.days.remove(&day);
        self
    }

    pub fn is_working_day(&self, day: Weekday) -> bool {
        self.days.contains_key(&day)
    }

    pub fn hours_on(&self, day: Weekday) -> f32 {
        self.days.get(&day).copied().unwrap_or(0.0)
    }

    pub fn total_hours_per_week(&self) -> f32 {
        self.days.values().sum()
    }

    pub fn working_days_per_week(&self) -> f32 {
        self.days.values().filter(|&&h| h > 0.0).count() as f32
    }

    pub fn hours_per_workload_day(&self) -> f32 {
        if self.days.is_empty() {
            return 8.0;
        }
        use std::collections::HashMap;
        let mut counts: HashMap<u32, (f32, u32)> = HashMap::new();
        for &h in self.days.values() {
            if h > 0.0 {
                let bits = h.to_bits();
                let entry = counts.entry(bits).or_insert((h, 0));
                entry.1 += 1;
            }
        }
        if counts.is_empty() {
            return 8.0;
        }
        let max_freq = counts.values().map(|&(_, c)| c).max().unwrap_or(0);
        counts
            .values()
            .filter(|&&(_, c)| c == max_freq)
            .map(|&(h, _)| h)
            .fold(f32::NEG_INFINITY, f32::max)
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Default for WorkSchedule {
    fn default() -> Self {
        Self::weekdays()
    }
}
// }}}
