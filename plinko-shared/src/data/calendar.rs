use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalendarOverrides {
    pub entries: HashMap<NaiveDate, f32>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl CalendarOverrides {
    pub fn set(&mut self, date: NaiveDate, hours: f32) {
        self.entries.insert(date, hours);
    }

    pub fn remove(&mut self, date: &NaiveDate) {
        self.entries.remove(date);
    }

    pub fn get(&self, date: NaiveDate) -> Option<f32> {
        self.entries.get(&date).copied()
    }
}
// }}}
