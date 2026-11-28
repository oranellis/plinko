// modules{{{
use std::fmt::Debug;

use crate::availability::workload::Workload;
// }}}

/// Hours of availability, or whether a day is allowed
pub trait DayValue: Copy + Debug + Default + PartialEq + 'static {
    /// Counts toward finite workday runs
    fn is_working(&self) -> bool;

    /// Monday first
    fn default_week() -> [Self; 7];
}

// impl DayValue {{{
impl DayValue for Workload {
    fn is_working(&self) -> bool {
        self.get() >= 1
    }

    fn default_week() -> [Self; 7] {
        [8, 8, 8, 8, 8, 0, 0].map(Workload::new_const)
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
