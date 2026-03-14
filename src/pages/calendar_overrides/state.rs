//! Mutable state for the calendar overrides page.

use crate::data::ids::NodeId;
use chrono::{Datelike, Local, NaiveDate};

pub struct CalendarOverridesState {
    pub toolbar_btn_hovered: Option<usize>,
    /// Currently displayed year.
    pub year: i32,
    /// Currently displayed month (1–12).
    pub month: u32,
    /// Day cell currently under the cursor.
    pub hovered_date: Option<NaiveDate>,
    /// Day cell being edited (None = no popup open).
    pub editing_date: Option<NaiveDate>,
    /// Text in the inline edit input.
    pub edit_input: String,
    /// True if the last parse attempt failed.
    pub edit_error: bool,
    pub open_settings_window: bool,
    pub settings_init_name: String,
    pub settings_init_date: String,
    pub settings_init_scheduler_target: NodeId,
}

impl CalendarOverridesState {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            toolbar_btn_hovered: None,
            year: today.year(),
            month: today.month(),
            hovered_date: None,
            editing_date: None,
            edit_input: String::new(),
            edit_error: false,
            open_settings_window: false,
            settings_init_name: String::new(),
            settings_init_date: String::new(),
            settings_init_scheduler_target: NodeId::PlanStart,
        }
    }

    pub fn prev_month(&mut self) {
        if self.month == 1 {
            self.month = 12;
            self.year -= 1;
        } else {
            self.month -= 1;
        }
    }

    pub fn next_month(&mut self) {
        if self.month == 12 {
            self.month = 1;
            self.year += 1;
        } else {
            self.month += 1;
        }
    }
}
