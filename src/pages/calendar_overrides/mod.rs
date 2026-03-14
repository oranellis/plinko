//! Calendar overrides page — edit per-date hour exceptions for the plan.

pub mod render;
pub mod state;

use skia_safe::Canvas;
use winit::keyboard::{Key, NamedKey};

use crate::data::Plan;
use crate::engine::{PlanRequest, PlanRequestSender};
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
use crate::ui::layout::{TOOLBAR_BTN_SIZE, TOOLBAR_BTN_Y};

use state::CalendarOverridesState;

pub struct CalendarOverridesPage {
    pub state: CalendarOverridesState,
}

impl CalendarOverridesPage {
    pub fn new() -> Self {
        Self {
            state: CalendarOverridesState::new(),
        }
    }
}

impl Page for CalendarOverridesPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        render::draw_calendar_overrides(canvas, width, height, &self.state, cache, plan);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        let new_hover = render::hit_test_toolbar_buttons(x, y, width);
        let toolbar_dirty = new_hover != self.state.toolbar_btn_hovered;
        if toolbar_dirty {
            self.state.toolbar_btn_hovered = new_hover;
        }

        let new_day = if y > TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE {
            render::hit_test_day(x, y, width, height, self.state.year, self.state.month)
        } else {
            None
        };
        let day_dirty = new_day != self.state.hovered_date;
        if day_dirty {
            self.state.hovered_date = new_day;
        }

        if toolbar_dirty || day_dirty {
            DirtyRegion::PageOnly
        } else {
            DirtyRegion::None
        }
    }

    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        plan: &Plan,
        sender: &PlanRequestSender,
    ) -> DirtyRegion {
        if !pressed {
            return DirtyRegion::None;
        }

        // Click outside popup closes it
        if let Some(date) = self.state.editing_date
            && render::hit_test_day(x, y, width, height, self.state.year, self.state.month)
                != Some(date)
        {
            self.state.editing_date = None;
            self.state.edit_input.clear();
            self.state.edit_error = false;
            return DirtyRegion::PageOnly;
        }

        // Toolbar
        if let Some(btn) = render::hit_test_toolbar_buttons(x, y, width) {
            match btn {
                0 => self.state.prev_month(),
                1 => self.state.next_month(),
                2 => {
                    self.state.settings_init_name = plan.name.clone();
                    self.state.settings_init_date = plan.start_date.to_string();
                    self.state.settings_init_scheduler_target = plan.scheduler_target;
                    self.state.open_settings_window = true;
                }
                _ => {}
            }
            return DirtyRegion::PageOnly;
        }

        // Day cell click
        if y > TOOLBAR_BTN_Y + TOOLBAR_BTN_SIZE
            && let Some(date) =
                render::hit_test_day(x, y, width, height, self.state.year, self.state.month)
        {
            // If already editing same cell — try submit (same as Enter)
            if self.state.editing_date == Some(date) {
                try_commit(&mut self.state, sender);
            } else {
                // Open edit popup with current value
                let current = plan
                    .calendar
                    .get(date)
                    .map(|h| format!("{h}"))
                    .unwrap_or_default();
                self.state.editing_date = Some(date);
                self.state.edit_input = current;
                self.state.edit_error = false;
            }
            return DirtyRegion::PageOnly;
        }

        DirtyRegion::None
    }

    fn on_key_input(&mut self, key: &Key, sender: &PlanRequestSender) -> DirtyRegion {
        if self.state.editing_date.is_none() {
            return DirtyRegion::None;
        }

        match key {
            Key::Named(NamedKey::Escape) => {
                self.state.editing_date = None;
                self.state.edit_input.clear();
                self.state.edit_error = false;
                DirtyRegion::PageOnly
            }
            Key::Named(NamedKey::Enter) => {
                try_commit(&mut self.state, sender);
                DirtyRegion::PageOnly
            }
            Key::Named(NamedKey::Backspace) => {
                self.state.edit_input.pop();
                self.state.edit_error = false;
                DirtyRegion::PageOnly
            }
            Key::Character(ch) => {
                for c in ch.chars() {
                    if c.is_ascii_digit() || c == '.' {
                        self.state.edit_input.push(c);
                    }
                }
                self.state.edit_error = false;
                DirtyRegion::PageOnly
            }
            _ => DirtyRegion::None,
        }
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if self.state.open_settings_window {
            self.state.open_settings_window = false;
            let w = crate::ui::plan_settings_window::PlanSettingsWindow::with_values(
                &self.state.settings_init_name,
                &self.state.settings_init_date,
                self.state.settings_init_scheduler_target,
            );
            return Some(Box::new(w));
        }
        None
    }

    fn reset_hover(&mut self) {
        self.state.toolbar_btn_hovered = None;
        self.state.hovered_date = None;
    }
}

/// Parse `edit_input`, send the appropriate engine request, and close the popup.
fn try_commit(state: &mut CalendarOverridesState, sender: &PlanRequestSender) {
    if let Some(date) = state.editing_date {
        let trimmed = state.edit_input.trim();
        if trimmed.is_empty() {
            // Clear the override
            sender.send(PlanRequest::ClearCalendarOverride(date));
            state.editing_date = None;
            state.edit_input.clear();
            state.edit_error = false;
        } else if let Ok(hours) = trimmed.parse::<f32>() {
            if hours >= 0.0 {
                sender.send(PlanRequest::SetCalendarOverride(date, hours));
                state.editing_date = None;
                state.edit_input.clear();
                state.edit_error = false;
            } else {
                state.edit_error = true;
            }
        } else {
            state.edit_error = true;
        }
    }
}
