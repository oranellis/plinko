//! Settings page — plan management and identity (current user) selection.

pub mod render;
pub mod state;

use skia_safe::{Canvas, Contains, Point};
use uuid::Uuid;

use crate::engine::PlanRequestSender;
use crate::pages::Page;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use plinko_shared::data::Plan;

use render::{
    CONTENT_TOP, ROW_H, identity_section_y, load_btn_rect, monday_btn_rect, new_btn_rect,
    plan_box_rect, plan_list_max_scroll, plan_row_rect, save_btn_rect, total_content_height,
    user_row_rect,
};
use state::SettingsState;

/// Settings page: manage plans and set the "current user" identity.
pub struct SettingsPage {
    pub state: SettingsState,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl SettingsPage {
    pub fn new() -> Self {
        Self {
            state: SettingsState::default(),
        }
    }

    fn max_scroll(plan: &Plan, state: &SettingsState, height: f32) -> f32 {
        let content_h = total_content_height(plan, &state.plan_list);
        let viewport_h = height - CONTENT_TOP;
        (content_h - viewport_h).max(0.0)
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl Page for SettingsPage {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        render::draw_settings(canvas, width, height, &self.state, plan, cache);
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        let mut dirty = false;
        self.state.cursor_y = y;

        let in_save = save_btn_rect(width).contains(Point::new(x, y));
        if in_save != self.state.hovered_save {
            self.state.hovered_save = in_save;
            dirty = true;
        }
        let in_new = new_btn_rect(width).contains(Point::new(x, y));
        if in_new != self.state.hovered_new {
            self.state.hovered_new = in_new;
            dirty = true;
        }
        let in_monday = monday_btn_rect(width).contains(Point::new(x, y));
        if in_monday != self.state.hovered_monday {
            self.state.hovered_monday = in_monday;
            dirty = true;
        }

        // Plan rows — hover only counts if cursor is inside the box
        let box_rect = plan_box_rect(width);
        let mut new_hov_row = None;
        let mut new_hov_load = None;
        if box_rect.contains(Point::new(x, y)) {
            for idx in 0..self.state.plan_list.len() {
                if load_btn_rect(idx, self.state.plan_list_scroll_y, width)
                    .contains(Point::new(x, y))
                {
                    new_hov_load = Some(idx);
                    new_hov_row = Some(idx);
                } else if plan_row_rect(idx, self.state.plan_list_scroll_y, width)
                    .contains(Point::new(x, y))
                {
                    new_hov_row = Some(idx);
                }
            }
        }
        if new_hov_row != self.state.hovered_plan_row {
            self.state.hovered_plan_row = new_hov_row;
            dirty = true;
        }
        if new_hov_load != self.state.hovered_load_btn {
            self.state.hovered_load_btn = new_hov_load;
            dirty = true;
        }

        // User rows
        let users_len = _plan.users_data.len();
        let total_user_rows = users_len + 1;
        let ident_y = identity_section_y(self.state.scroll_y);
        let rows_top = ident_y + 20.0 /* SECTION_TITLE_H */ + 12.0 /* SECTION_GAP */;
        let mut new_hov_user = None;
        for idx in 0..total_user_rows {
            let row_y = rows_top + idx as f32 * ROW_H;
            let row_rect = skia_safe::Rect::from_xywh(16.0, row_y, width - 32.0, ROW_H);
            if row_rect.contains(Point::new(x, y)) {
                new_hov_user = Some(idx);
                break;
            }
        }
        if new_hov_user != self.state.hovered_user_idx {
            self.state.hovered_user_idx = new_hov_user;
            dirty = true;
        }

        if dirty {
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
        _height: f32,
        plan: &Plan,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        if !pressed {
            return DirtyRegion::None;
        }

        if save_btn_rect(width).contains(Point::new(x, y)) {
            self.state.pending_save = true;
            return DirtyRegion::PageOnly;
        }
        if new_btn_rect(width).contains(Point::new(x, y)) {
            self.state.pending_new = true;
            return DirtyRegion::PageOnly;
        }
        if monday_btn_rect(width).contains(Point::new(x, y)) {
            self.state.pending_open_monday = true;
            return DirtyRegion::PageOnly;
        }

        // Plan rows — only hit-test if click is within the box
        let box_rect = plan_box_rect(width);
        if box_rect.contains(Point::new(x, y)) {
            for idx in 0..self.state.plan_list.len() {
                let entry = &self.state.plan_list[idx];
                if !entry.is_current
                    && plan_row_rect(idx, self.state.plan_list_scroll_y, width)
                        .contains(Point::new(x, y))
                {
                    let id: Uuid = entry.id;
                    self.state.pending_load = Some(id);
                    return DirtyRegion::PageOnly;
                }
            }
            return DirtyRegion::None;
        }

        // User rows
        let users = {
            let mut v: Vec<_> = plan
                .users_data
                .iter()
                .map(|(id, ud)| (*id, ud.user.name.clone()))
                .collect();
            v.sort_by(|a, b| a.1.cmp(&b.1));
            v
        };
        let total_user_rows = users.len() + 1;
        let ident_y = identity_section_y(self.state.scroll_y);
        let rows_top = ident_y + 20.0 + 12.0;
        for idx in 0..total_user_rows {
            let row_y = rows_top + idx as f32 * ROW_H;
            let row_rect = skia_safe::Rect::from_xywh(16.0, row_y, width - 32.0, ROW_H);
            if row_rect.contains(Point::new(x, y)) {
                let uid = users.get(idx).map(|(id, _)| *id);
                self.state.pending_set_user = Some(uid);
                self.state.current_user = uid;
                return DirtyRegion::PageOnly;
            }
        }

        DirtyRegion::None
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _shift: bool,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion {
        let box_rect = plan_box_rect(width);
        if self.state.cursor_y >= box_rect.top && self.state.cursor_y <= box_rect.bottom {
            // Scroll the plan list box
            let max = plan_list_max_scroll(self.state.plan_list.len());
            let new_scroll = (self.state.plan_list_scroll_y - delta_y * 40.0).clamp(0.0, max);
            if (new_scroll - self.state.plan_list_scroll_y).abs() > 0.5 {
                self.state.plan_list_scroll_y = new_scroll;
                return DirtyRegion::PageOnly;
            }
            return DirtyRegion::None;
        }
        let max = Self::max_scroll(plan, &self.state, height);
        let new_scroll = (self.state.scroll_y - delta_y * 40.0).clamp(0.0, max);
        if (new_scroll - self.state.scroll_y).abs() > 0.5 {
            self.state.scroll_y = new_scroll;
            DirtyRegion::PageOnly
        } else {
            DirtyRegion::None
        }
    }

    fn reset_hover(&mut self) {
        self.state.hovered_save = false;
        self.state.hovered_new = false;
        self.state.hovered_monday = false;
        self.state.hovered_plan_row = None;
        self.state.hovered_load_btn = None;
        self.state.hovered_user_idx = None;
    }
}
// }}}
