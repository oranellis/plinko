//! Floating window that shows a scrollable list of all team members.

use winit::event::Modifiers;

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Matrix, Paint, PaintStyle, PathBuilder, Point, RRect, Rect,
    TextBlob,
};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_SIZE, DIVIDER_COLOR, ITEM_FG, LIST_BG, LIST_ITEM_HOVER_BG,
    LIST_SECTION_FG, OVERLAY_SOFT, PANEL_BG, PANEL_TEXT, PLAN_LIST_ITEM_H, SCROLLBAR_THUMB_COLOR,
    TOOLBAR_BTN_HOVER_BG, TOOLBAR_BTN_ICON_COLOR, TOOLBAR_STROKE_WIDTH,
};
use plinko_shared::data::Plan;

const PANEL_W: f32 = 420.0;
const PANEL_H: f32 = 520.0;
const TITLE_H: f32 = 48.0;
const ROW_H: f32 = PLAN_LIST_ITEM_H;
const PADDING: f32 = 16.0;
/// Inset from the panel edge to the title-bar button (matches vertical centering).
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const BTN_GAP: f32 = 4.0;
const CORNER: f32 = 8.0;
const SCROLLBAR_W: f32 = 4.0;

/// Floating window displaying a scrollable list of all [`User`]s in the plan.
pub struct UsersWindow {
    scroll_offset: f32,
    hovered_back: bool,
    hovered_plus: bool,
    hovered_tags: bool,
    hovered_row: Option<usize>,
    hovered_cal_btn: Option<usize>,
    pending_open_add: bool,
    pending_open_tags: bool,
    pending_edit: Option<plinko_shared::data::User>,
    pending_schedule: Option<Box<dyn crate::ui::floating_window::FloatingWindow>>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl UsersWindow {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            hovered_back: false,
            hovered_plus: false,
            hovered_tags: false,
            hovered_row: None,
            hovered_cal_btn: None,
            pending_open_add: false,
            pending_open_tags: false,
            pending_edit: None,
            pending_schedule: None,
        }
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph)
    }

    /// Back (chevron) button — left side of title bar.
    fn back_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.left + BTN_INSET,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    /// Tags button — second from right in title bar.
    fn tags_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.right - BTN_INSET - BACK_BTN_SIZE - BTN_GAP - BACK_BTN_SIZE,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    /// Plus button — rightmost button in title bar.
    fn plus_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.right - BTN_INSET - BACK_BTN_SIZE,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn list_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        // Subtract 1 px for the divider line
        Rect::from_xywh(
            panel.left,
            panel.top + TITLE_H + 1.0,
            panel.width(),
            panel.height() - TITLE_H - 1.0,
        )
    }

    fn row_y(&self, list_top: f32, idx: usize) -> f32 {
        list_top + idx as f32 * ROW_H - self.scroll_offset
    }

    fn cal_btn_rect(row_y: f32, panel: Rect) -> Rect {
        const CAL_BTN_SIZE: f32 = 24.0;
        Rect::from_xywh(
            panel.right - PADDING - SCROLLBAR_W - 4.0 - CAL_BTN_SIZE,
            row_y + (ROW_H - CAL_BTN_SIZE) / 2.0,
            CAL_BTN_SIZE,
            CAL_BTN_SIZE,
        )
    }

    fn max_scroll(user_count: usize, width: f32, height: f32) -> f32 {
        let content_h = user_count as f32 * ROW_H;
        let list_h = Self::list_rect(width, height).height();
        (content_h - list_h).max(0.0)
    }

    fn hovered_row_for(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        user_count: usize,
    ) -> Option<usize> {
        let list = Self::list_rect(width, height);
        if !list.contains(Point::new(x, y)) {
            return None;
        }
        let rel_y = y - list.top + self.scroll_offset;
        let idx = (rel_y / ROW_H) as usize;
        if idx < user_count { Some(idx) } else { None }
    }

    /// Draws a generic icon button (plus or tag) at `btn_rect`.
    fn draw_icon_btn(canvas: &Canvas, btn_rect: Rect, hovered: bool, icon: &skia_safe::Path) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        if hovered {
            paint.set_color(Color::from(TOOLBAR_BTN_HOVER_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(btn_rect, BACK_BTN_CORNER, BACK_BTN_CORNER),
                &paint,
            );
        }

        let icon_draw_size = BACK_BTN_SIZE * 0.5;
        let offset_x = btn_rect.left + (BACK_BTN_SIZE - icon_draw_size) / 2.0;
        let offset_y = btn_rect.top + (BACK_BTN_SIZE - icon_draw_size) / 2.0;

        let bounds = icon.bounds();
        let src_w = bounds.width().max(1.0);
        let src_h = bounds.height().max(1.0);
        let scale_x = icon_draw_size / src_w;
        let scale_y = icon_draw_size / src_h;
        let tx = offset_x - bounds.left * scale_x;
        let ty = offset_y - bounds.top * scale_y;

        let matrix = Matrix::scale_translate((scale_x, scale_y), (tx, ty));
        let scaled = icon.with_transform(&matrix);

        paint.set_color(Color::from(TOOLBAR_BTN_ICON_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
        canvas.draw_path(&scaled, &paint);
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl FloatingWindow for UsersWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let list = Self::list_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let plus_btn = Self::plus_btn_rect(width, height);
        let tags_btn = Self::tags_btn_rect(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from(OVERLAY_SOFT));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(
                    panel.left + 2.0,
                    panel.top + 4.0,
                    panel.width(),
                    panel.height(),
                ),
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Title bar background — full rrect then square off the bottom half
        let title_rect = Rect::from_xywh(panel.left, panel.top, panel.width(), TITLE_H);
        paint.set_color(Color::from(LIST_BG));
        canvas.draw_rrect(RRect::new_rect_xy(title_rect, CORNER, CORNER), &paint);
        canvas.draw_rect(
            Rect::from_xywh(
                panel.left,
                panel.top + CORNER,
                panel.width(),
                TITLE_H - CORNER,
            ),
            &paint,
        );

        // Title text — horizontally centered in the title bar
        if let Some(blob) = TextBlob::new("Team Members", &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str("Team Members", None);
            let tx = panel.left + (panel.width() - advance) / 2.0;
            let ty =
                panel.top + (TITLE_H - metrics.descent + metrics.ascent) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Back (chevron) button
        crate::ui::window_chrome::draw_chevron_btn(canvas, back_btn, self.hovered_back);

        // Plus button
        Self::draw_icon_btn(canvas, plus_btn, self.hovered_plus, &cache.icon_plus);

        // Tags button
        Self::draw_icon_btn(canvas, tags_btn, self.hovered_tags, &cache.icon_tag);

        // Divider below title
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // --- List area ---
        canvas.save();
        canvas.clip_rect(list, ClipOp::Intersect, false);

        let mut sorted_users: Vec<_> = plan
            .users_data
            .values()
            .map(|ud| &ud.user)
            .collect::<Vec<_>>();
        sorted_users.sort_by(|a, b| a.name.cmp(&b.name));

        if sorted_users.is_empty() {
            if let Some(blob) = TextBlob::new("No team members yet", &cache.font) {
                let (_, metrics) = cache.font.metrics();
                let (advance, _) = cache.font.measure_str("No team members yet", None);
                let tx = panel.left + (panel.width() - advance) / 2.0;
                let ty = list.top + 48.0 - metrics.ascent;
                paint.set_color(Color::from(PANEL_TEXT));
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
        } else {
            let (_, metrics) = cache.font.metrics();
            let row_text_offset =
                (ROW_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

            let (_, sm_metrics) = cache.small_font.metrics();
            let sm_row_text_offset =
                (ROW_H - (sm_metrics.descent - sm_metrics.ascent)) / 2.0 - sm_metrics.ascent;

            let sorted_len = sorted_users.len();
            for (i, user) in sorted_users.iter().enumerate() {
                let ry = self.row_y(list.top, i);

                // Skip rows entirely outside the visible list area
                if ry + ROW_H < list.top || ry > list.bottom {
                    continue;
                }

                // Row hover background
                if self.hovered_row == Some(i) {
                    paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                    canvas.draw_rect(
                        Rect::from_xywh(panel.left, ry, panel.width(), ROW_H),
                        &paint,
                    );
                }

                // Name
                if let Some(blob) = TextBlob::new(&user.name, &cache.font) {
                    paint.set_color(Color::from(ITEM_FG));
                    canvas.draw_text_blob(
                        &blob,
                        (panel.left + PADDING, ry + row_text_offset),
                        &paint,
                    );
                }

                // Tags (right-aligned, small font, sorted by name)
                if !user.tags.is_empty() {
                    let mut tag_names: Vec<&str> = user
                        .tags
                        .iter()
                        .filter_map(|id| {
                            plan.tags
                                .iter()
                                .find(|t| &t.id == id)
                                .map(|t| t.name.as_str())
                        })
                        .collect();
                    tag_names.sort_unstable();
                    let tags_str = tag_names.join(", ");
                    if let Some(blob) = TextBlob::new(&tags_str, &cache.small_font) {
                        let tx = panel.right
                            - PADDING
                            - SCROLLBAR_W
                            - 4.0
                            - 24.0  // cal btn size
                            - 8.0   // gap
                            - blob.bounds().width();
                        paint.set_color(Color::from(LIST_SECTION_FG));
                        canvas.draw_text_blob(&blob, (tx, ry + sm_row_text_offset), &paint);
                    }
                }

                // Calendar button (per-row)
                {
                    let cal_btn = Self::cal_btn_rect(ry, panel);
                    let hov = self.hovered_cal_btn == Some(i);
                    if hov {
                        paint.set_color(Color::from(TOOLBAR_BTN_HOVER_BG));
                        paint.set_style(PaintStyle::Fill);
                        canvas.draw_rrect(RRect::new_rect_xy(cal_btn, 4.0, 4.0), &paint);
                    }
                    // Simple calendar icon
                    let icon_color = if hov {
                        TOOLBAR_BTN_ICON_COLOR
                    } else {
                        0xff_bbbbbb_u32
                    };
                    let x = cal_btn.left + 3.0;
                    let y = cal_btn.top + 4.0;
                    let w = cal_btn.width() - 6.0;
                    let h = cal_btn.height() - 7.0;
                    paint.set_color(Color::from(icon_color));
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.2);
                    // Outer rounded rect
                    canvas.draw_rrect(
                        RRect::new_rect_xy(Rect::from_xywh(x, y, w, h), 2.0, 2.0),
                        &paint,
                    );
                    // Binding tabs at top
                    let mut pb = PathBuilder::new();
                    pb.move_to((x + 3.0, y));
                    pb.line_to((x + 3.0, cal_btn.top + 2.0));
                    pb.move_to((x + w - 3.0, y));
                    pb.line_to((x + w - 3.0, cal_btn.top + 2.0));
                    canvas.draw_path(&pb.detach(), &paint);
                    // Horizontal lines (calendar grid)
                    let row_h = (h - 5.0) / 3.0;
                    let lines_top = y + 5.0;
                    paint.set_stroke_width(0.8);
                    for li in 0..3 {
                        let ly = lines_top + li as f32 * row_h;
                        canvas.draw_line((x + 1.0, ly), (x + w - 1.0, ly), &paint);
                    }
                    paint.set_style(PaintStyle::Fill);
                }

                // Row divider (except after last row)
                if i + 1 < sorted_len {
                    paint.set_color(Color::from(DIVIDER_COLOR));
                    canvas.draw_rect(
                        Rect::from_xywh(
                            panel.left + PADDING,
                            ry + ROW_H - 1.0,
                            panel.width() - 2.0 * PADDING,
                            1.0,
                        ),
                        &paint,
                    );
                }
            }
        }

        canvas.restore();

        // Scrollbar thumb
        let user_count = sorted_users.len();
        drop(sorted_users);
        let max_scroll = Self::max_scroll(user_count, width, height);
        if max_scroll > 0.0 {
            let list_h = list.height();
            let content_h = user_count as f32 * ROW_H;
            let thumb_h = (list_h * list_h / content_h).max(20.0);
            let thumb_y = list.top + (self.scroll_offset / max_scroll) * (list_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        panel.right - SCROLLBAR_W - 2.0,
                        thumb_y,
                        SCROLLBAR_W,
                        thumb_h,
                    ),
                    2.0,
                    2.0,
                ),
                &paint,
            );
        }
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> FloatingWindowOutcome {
        let pt = Point::new(x, y);
        let new_back = Self::back_btn_rect(width, height).contains(pt);
        let new_plus = Self::plus_btn_rect(width, height).contains(pt);
        let new_tags = Self::tags_btn_rect(width, height).contains(pt);
        let new_row = self.hovered_row_for(x, y, width, height, plan.users_data.len());

        // Check calendar button hover
        let panel = Self::panel_rect(width, height);
        let list = Self::list_rect(width, height);
        let mut sorted_users: Vec<_> = plan
            .users_data
            .values()
            .map(|ud| &ud.user)
            .collect::<Vec<_>>();
        sorted_users.sort_by(|a, b| a.name.cmp(&b.name));
        let new_cal_btn = sorted_users.iter().enumerate().find_map(|(i, _)| {
            let ry = self.row_y(list.top, i);
            if ry + ROW_H < list.top || ry > list.bottom {
                return None;
            }
            let btn = Self::cal_btn_rect(ry, panel);
            if btn.contains(pt) { Some(i) } else { None }
        });

        if new_back != self.hovered_back
            || new_plus != self.hovered_plus
            || new_tags != self.hovered_tags
            || new_row != self.hovered_row
            || new_cal_btn != self.hovered_cal_btn
        {
            self.hovered_back = new_back;
            self.hovered_plus = new_plus;
            self.hovered_tags = new_tags;
            self.hovered_row = new_row;
            self.hovered_cal_btn = new_cal_btn;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        _modifiers: &Modifiers,
        plan: &Plan,
        _sender: &PlanRequestSender,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let pt = Point::new(x, y);
        // Back button closes the window
        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        // Plus button — open the Add Team Member form
        if Self::plus_btn_rect(width, height).contains(pt) {
            self.pending_open_add = true;
            return FloatingWindowOutcome::default();
        }
        // Tags button — open the Tags window
        if Self::tags_btn_rect(width, height).contains(pt) {
            self.pending_open_tags = true;
            return FloatingWindowOutcome::default();
        }
        // Click outside the panel closes it
        if !Self::panel_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }

        // Calendar button check (before row click so the btn hit-tests first)
        {
            let panel = Self::panel_rect(width, height);
            let list = Self::list_rect(width, height);
            let mut sorted_users: Vec<_> = plan
                .users_data
                .values()
                .map(|ud| &ud.user)
                .collect::<Vec<_>>();
            sorted_users.sort_by(|a, b| a.name.cmp(&b.name));
            for (i, user) in sorted_users.iter().enumerate() {
                let ry = self.row_y(list.top, i);
                if ry + ROW_H < list.top || ry > list.bottom {
                    continue;
                }
                let cal_btn = Self::cal_btn_rect(ry, panel);
                if cal_btn.contains(pt) {
                    let user_schedule = plan.schedule_for(&user.id).clone();
                    self.pending_schedule = Some(Box::new(
                        crate::ui::schedule_window::ScheduleWindow::for_user(
                            user.id,
                            &user.name,
                            &user_schedule,
                        ),
                    ));
                    return FloatingWindowOutcome::default();
                }
            }
        }

        // Row click — open the Edit Team Member form
        if let Some(idx) = self.hovered_row_for(x, y, width, height, plan.users_data.len()) {
            let mut sorted_users: Vec<_> = plan
                .users_data
                .values()
                .map(|ud| &ud.user)
                .collect::<Vec<_>>();
            sorted_users.sort_by(|a, b| a.name.cmp(&b.name));
            if let Some(user) = sorted_users.get(idx) {
                self.pending_edit = Some((*user).clone());
                return FloatingWindowOutcome::default();
            }
        }
        FloatingWindowOutcome::default()
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        let max = Self::max_scroll(plan.users_data.len(), width, height);
        if max <= 0.0 {
            return FloatingWindowOutcome::default();
        }
        let new_offset = (self.scroll_offset - delta_y * 40.0).clamp(0.0, max);
        if (new_offset - self.scroll_offset).abs() > f32::EPSILON {
            self.scroll_offset = new_offset;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if let Some(w) = self.pending_schedule.take() {
            return Some(w);
        }
        if let Some(user) = self.pending_edit.take() {
            return Some(Box::new(
                crate::ui::user_form_window::UserFormWindow::from_user(&user),
            ));
        }
        if self.pending_open_add {
            self.pending_open_add = false;
            return Some(Box::new(crate::ui::user_form_window::UserFormWindow::new()));
        }
        if self.pending_open_tags {
            self.pending_open_tags = false;
            return Some(Box::new(crate::ui::tags_window::TagsWindow::new()));
        }
        None
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_plus = false;
        self.hovered_tags = false;
        self.hovered_row = None;
        self.hovered_cal_btn = None;
    }
}
// }}}
