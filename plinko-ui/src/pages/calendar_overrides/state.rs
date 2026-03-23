//! Mutable state for the calendar overrides page.

use chrono::{Datelike, Local, NaiveDate};
use plinko_shared::data::ids::{NodeId, UserId};
use skia_safe::Rect;
use std::cell::RefCell;

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
    /// The currently selected user for per-user overrides.
    /// `None` = plan-wide overrides.
    pub selected_user: Option<UserId>,
    /// The signed-in user (set from app.rs).
    pub current_user: Option<UserId>,
    /// Index of hovered quick-selector button (-1 = unused, ≥0 = index).
    pub hovered_user_tab: Option<i32>,
    /// Whether the "other users" dropdown is open.
    pub user_dropdown_open: bool,
    /// Filter string typed in the dropdown.
    pub user_filter: String,
    /// Index of the hovered item in the dropdown list.
    pub hovered_dropdown_item: Option<usize>,
    /// Cached tab rects from the last render, used for hit testing.
    pub tab_rects: RefCell<Vec<Rect>>,
    /// Cached x position where the dropdown button starts.
    pub dropdown_btn_x_cached: RefCell<f32>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
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
            selected_user: None,
            current_user: None,
            hovered_user_tab: None,
            user_dropdown_open: false,
            user_filter: String::new(),
            hovered_dropdown_item: None,
            tab_rects: RefCell::new(Vec::new()),
            dropdown_btn_x_cached: RefCell::new(0.0),
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
// }}}
