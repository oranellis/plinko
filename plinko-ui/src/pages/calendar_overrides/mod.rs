//! Calendar overrides page — edit per-date hour exceptions for the plan.

pub mod render;
pub mod state;

use skia_safe::Canvas;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindow;
use crate::ui::layout::{TOOLBAR_BTN_SIZE, TOOLBAR_BTN_Y};
use plinko_shared::data::Plan;
use plinko_shared::data::ids::UserId;
use plinko_shared::protocol::PlanRequest;

use state::CalendarOverridesState;

pub struct CalendarOverridesPage {
    pub state: CalendarOverridesState,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl CalendarOverridesPage {
    pub fn new() -> Self {
        Self {
            state: CalendarOverridesState::new(),
        }
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
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
        plan: &Plan,
    ) -> DirtyRegion {
        let new_hover = render::hit_test_toolbar_buttons(x, y, width);
        let toolbar_dirty = new_hover != self.state.toolbar_btn_hovered;
        if toolbar_dirty {
            self.state.toolbar_btn_hovered = new_hover;
        }

        // User selector tab hover
        let new_tab_hover = render::hit_test_user_tab(x, y, &self.state).map(|i| i as i32);
        let tab_dirty = new_tab_hover != self.state.hovered_user_tab;
        if tab_dirty {
            self.state.hovered_user_tab = new_tab_hover;
        }

        // Dropdown item hover
        let mut drop_dirty = false;
        if self.state.user_dropdown_open {
            let btn_x = render::dropdown_btn_x(&self.state);
            let new_drop_hover = render::hit_test_dropdown_item(
                x,
                y,
                plan,
                self.state.current_user,
                &self.state.user_filter,
                btn_x,
                width,
            );
            if new_drop_hover != self.state.hovered_dropdown_item {
                self.state.hovered_dropdown_item = new_drop_hover;
                drop_dirty = true;
            }
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

        if toolbar_dirty || day_dirty || tab_dirty || drop_dirty {
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

        // If dropdown is open, handle dropdown interactions first.
        if self.state.user_dropdown_open {
            let btn_x = render::dropdown_btn_x(&self.state);

            // Click inside filter box — handled by key_input; just close nothing
            if render::hit_test_dropdown_filter(x, y, &self.state, width) {
                return DirtyRegion::None;
            }

            // Click on a dropdown item
            if let Some(item_idx) = render::hit_test_dropdown_item(
                x,
                y,
                plan,
                self.state.current_user,
                &self.state.user_filter,
                btn_x,
                width,
            ) {
                let uid = render::user_for_dropdown_item(
                    item_idx,
                    plan,
                    self.state.current_user,
                    &self.state.user_filter,
                );
                self.state.selected_user = uid;
                self.state.user_dropdown_open = false;
                self.state.user_filter.clear();
                self.state.hovered_dropdown_item = None;
                self.state.editing_date = None;
                self.state.edit_input.clear();
                self.state.edit_error = false;
                return DirtyRegion::PageOnly;
            }

            // Click outside — close dropdown
            self.state.user_dropdown_open = false;
            self.state.user_filter.clear();
            self.state.hovered_dropdown_item = None;
            return DirtyRegion::PageOnly;
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

        // User selector quick buttons
        let others = render::other_users_count(plan, self.state.current_user);
        if let Some(tab_idx) = render::hit_test_user_tab(x, y, &self.state) {
            self.state.editing_date = None;
            self.state.edit_input.clear();
            self.state.edit_error = false;

            let has_current = self.state.current_user.is_some()
                && plan
                    .users_data
                    .contains_key(&self.state.current_user.unwrap());
            let dropdown_idx = if has_current { 2 } else { 1 };

            if others > 0 && tab_idx == dropdown_idx {
                // Toggle dropdown
                self.state.user_dropdown_open = !self.state.user_dropdown_open;
                self.state.user_filter.clear();
            } else if tab_idx == 0 {
                self.state.selected_user = None;
            } else if has_current && tab_idx == 1 {
                self.state.selected_user = self.state.current_user;
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
                // Open edit popup with current value.
                // Show user-specific override if present, else plan override.
                let current = self
                    .state
                    .selected_user
                    .as_ref()
                    .and_then(|uid| plan.user_calendar_overrides.get(uid))
                    .and_then(|c| c.get(date))
                    .or_else(|| plan.calendar.get(date))
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
        // Handle dropdown filter input
        if self.state.user_dropdown_open {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.state.user_dropdown_open = false;
                    self.state.user_filter.clear();
                    self.state.hovered_dropdown_item = None;
                    return DirtyRegion::PageOnly;
                }
                Key::Named(NamedKey::Backspace) => {
                    self.state.user_filter.pop();
                    return DirtyRegion::PageOnly;
                }
                Key::Character(ch) => {
                    self.state.user_filter.push_str(ch);
                    return DirtyRegion::PageOnly;
                }
                _ => {}
            }
        }

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
        self.state.hovered_user_tab = None;
    }
}
// }}}

/// Parse `edit_input`, send the appropriate engine request, and close the popup.
fn try_commit(state: &mut CalendarOverridesState, sender: &PlanRequestSender) {
    if let Some(date) = state.editing_date {
        let trimmed = state.edit_input.trim();
        if trimmed.is_empty() {
            // Clear the override
            if let Some(uid) = state.selected_user {
                sender.send(PlanRequest::ClearUserCalendarOverride(uid, date));
            } else {
                sender.send(PlanRequest::ClearCalendarOverride(date));
            }
            state.editing_date = None;
            state.edit_input.clear();
            state.edit_error = false;
        } else if let Ok(hours) = trimmed.parse::<f32>() {
            if hours >= 0.0 {
                if let Some(uid) = state.selected_user {
                    sender.send(PlanRequest::SetUserCalendarOverride(uid, date, hours));
                } else {
                    sender.send(PlanRequest::SetCalendarOverride(date, hours));
                }
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
