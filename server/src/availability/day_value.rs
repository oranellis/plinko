// modules{{{
use std::fmt::Debug;

use crate::availability::workload::Workload;
// }}}

/// What a rule set decides for each day: hours of availability, or whether a day is allowed
pub trait DayValue: Copy + Debug + Default + PartialEq + 'static {
    /// Whether a day counts as a working day for finite workday runs
    fn is_working(&self) -> bool;

    /// The weekly schedule in use until a rule replaces it, Monday first
    fn default_week() -> [Self; 7];
}

// impl DayValue {{{
impl DayValue for Workload {
    fn is_working(&self) -> bool {
        self.get() >= 1
    }

    fn default_week() -> [Self; 7] {
        [8, 8, 8, 8, 8, 0, 0].map(|h| Workload::new(h).expect("default hours are valid"))
    }
}

impl DayValue for bool {
    fn is_working(&self) -> bool {
        *self
    }

    fn default_week() -> [Self; 7] {
        [true; 7]
    }
}
// }}}
