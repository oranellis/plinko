//! Floating search window for navigating to a task or milestone on the Gantt chart.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use skia_safe::{Canvas, ClipOp, Color, Contains, Paint, PaintStyle, RRect, Rect, TextBlob};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, DIVIDER_COLOR, GHOST_FG, INPUT_BG, INPUT_BORDER, INPUT_BORDER_FOCUS,
    INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, ITEM_MILESTONE_DOT, ITEM_TASK_DOT, LIST_BG,
    LIST_ITEM_HOVER_BG, PLACEHOLDER_FG, PLAN_BTN_CORNER, SCROLLBAR_THUMB_COLOR,
};
use crate::ui::milestone_form_window::MilestoneFormWindow;
use crate::ui::task_form_window::TaskFormWindow;
use crate::ui::text_input::TextInput;
use crate::ui::window_chrome::draw_window_chrome;
use plinko_shared::data::Plan;
use plinko_shared::data::ids::NodeId;

const PANEL_W: f32 = 420.0;
const PANEL_H: f32 = 500.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const INPUT_H: f32 = 36.0;
const INPUT_PAD: f32 = 12.0;
const ROW_H: f32 = 36.0;
const BADGE_W: f32 = 76.0;
const SCROLLBAR_W: f32 = 4.0;

pub struct SearchWindow {
    filter: TextInput,
    scroll_y: f32,
    hovered_back: bool,
    hovered_row: Option<usize>,
    /// Cached filtered results from the last `render` call, for use in hit-testing.
    filter_results: RefCell<Vec<(NodeId, String, bool)>>,
    /// Shared result channel. When a node is selected, its `NodeId` is written here
    /// and the window closes. The overview page drains this in `tick_animation`.
    result: Arc<Mutex<Option<NodeId>>>,
    /// Set when a row is clicked; consumed by `take_open_request`.
    pending_edit: Option<Box<dyn FloatingWindow>>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl SearchWindow {
    pub fn new(result: Arc<Mutex<Option<NodeId>>>) -> Self {
        let mut filter = TextInput::new("");
        filter.focused = true;
        Self {
            filter,
            scroll_y: 0.0,
            hovered_back: false,
            hovered_row: None,
            filter_results: RefCell::new(Vec::new()),
            result,
            pending_edit: None,
        }
    }

    fn panel_rect(w: f32, h: f32) -> Rect {
        let pw = (w * 0.95).min(PANEL_W);
        let ph = (h * 0.95).min(PANEL_H);
        Rect::from_xywh((w - pw) / 2.0, (h - ph) / 2.0, pw, ph)
    }

    fn back_btn_rect(w: f32, h: f32) -> Rect {
        let p = Self::panel_rect(w, h);
        let inset = (TITLE_H - BACK_BTN_SIZE) / 2.0;
        Rect::from_xywh(p.left + inset, p.top + inset, BACK_BTN_SIZE, BACK_BTN_SIZE)
    }

    fn input_rect(w: f32, h: f32) -> Rect {
        let p = Self::panel_rect(w, h);
        Rect::from_xywh(
            p.left + INPUT_PAD,
            p.top + TITLE_H + INPUT_PAD,
            p.width() - 2.0 * INPUT_PAD,
            INPUT_H,
        )
    }

    fn list_rect(w: f32, h: f32) -> Rect {
        let p = Self::panel_rect(w, h);
        let list_top = p.top + TITLE_H + INPUT_PAD + INPUT_H + INPUT_PAD;
        Rect::from_xywh(p.left, list_top, p.width(), p.bottom - list_top)
    }

    fn compute_items(plan: &Plan, query: &str) -> Vec<(NodeId, String, bool)> {
        let q = query.to_lowercase();
        let mut items: Vec<(NodeId, String, bool)> = plan
            .tasks
            .iter()
            .filter(|(_, t)| q.is_empty() || t.name.to_lowercase().contains(&q))
            .map(|(id, t)| (NodeId::Task(*id), t.name.clone(), false))
            .chain(
                plan.milestones
                    .iter()
                    .filter(|(_, m)| q.is_empty() || m.name.to_lowercase().contains(&q))
                    .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone(), true)),
            )
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        items
    }
}
// }}}

// ── FloatingWindow ─────────────────────────────────────────────────────────── {{{
impl FloatingWindow for SearchWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        draw_window_chrome(
            canvas,
            panel,
            CORNER,
            TITLE_H,
            "Find",
            self.hovered_back,
            cache,
        );

        // ── Search input ──────────────────────────────────────────────────
        let input_r = Self::input_rect(width, height);
        draw_search_input(canvas, input_r, &self.filter, cache);

        // ── Filtered list ─────────────────────────────────────────────────
        let items = Self::compute_items(plan, &self.filter.content);
        *self.filter_results.borrow_mut() = items.clone();

        let list = Self::list_rect(width, height);
        let visible_h = list.height();
        let total_h = items.len() as f32 * ROW_H;

        canvas.save();
        canvas.clip_rect(list, ClipOp::Intersect, false);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let (_, metrics) = cache.font.metrics();
        let text_h = metrics.descent - metrics.ascent;

        for (i, (_, name, is_milestone)) in items.iter().enumerate() {
            let row_top = list.top + i as f32 * ROW_H - self.scroll_y;
            if row_top + ROW_H < list.top {
                continue;
            }
            if row_top > list.bottom {
                break;
            }

            // Row hover highlight
            if self.hovered_row == Some(i) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rect(
                    Rect::from_xywh(list.left, row_top, list.width(), ROW_H),
                    &paint,
                );
            }

            // Kind badge (right side)
            let badge_label = if *is_milestone { "Milestone" } else { "Task" };
            let badge_color = if *is_milestone {
                ITEM_MILESTONE_DOT
            } else {
                ITEM_TASK_DOT
            };
            let badge_x = list.right - BADGE_W - 8.0;
            let badge_top = row_top + (ROW_H - 20.0) / 2.0;
            let badge_rect = Rect::from_xywh(badge_x, badge_top, BADGE_W, 20.0);
            paint.set_color(Color::from(badge_color));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(RRect::new_rect_xy(badge_rect, 3.0, 3.0), &paint);

            if let Some(blob) = TextBlob::new(badge_label, &cache.small_font) {
                let (adv, _) = cache.small_font.measure_str(badge_label, None);
                let (_, sm) = cache.small_font.metrics();
                let bx = badge_rect.left + (badge_rect.width() - adv) / 2.0;
                let by = badge_rect.top + (badge_rect.height() - (sm.descent - sm.ascent)) / 2.0
                    - sm.ascent;
                paint.set_color(Color::from(0xff_ffffff));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (bx, by), &paint);
            }

            // Node name (clipped to avoid badge)
            let name_clip = Rect::from_xywh(
                list.left + 12.0,
                row_top,
                badge_x - 8.0 - list.left - 12.0,
                ROW_H,
            );
            canvas.save();
            canvas.clip_rect(name_clip, ClipOp::Intersect, false);
            if let Some(blob) = TextBlob::new(name, &cache.font) {
                let text_y = row_top + (ROW_H - text_h) / 2.0 - metrics.ascent;
                paint.set_color(Color::from(ITEM_FG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (list.left + 12.0, text_y), &paint);
            }
            canvas.restore();

            // Row divider
            paint.set_color(Color::from(DIVIDER_COLOR));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(
                Rect::from_xywh(
                    list.left + 8.0,
                    row_top + ROW_H - 1.0,
                    list.width() - 16.0,
                    1.0,
                ),
                &paint,
            );
        }

        if items.is_empty() {
            let msg = if self.filter.content.is_empty() {
                "No tasks or milestones in plan"
            } else {
                "No matching results"
            };
            if let Some(blob) = TextBlob::new(msg, &cache.font) {
                let (adv, _) = cache.font.measure_str(msg, None);
                let text_y = list.top + 48.0;
                paint.set_color(Color::from(GHOST_FG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(
                    &blob,
                    (list.left + (list.width() - adv) / 2.0, text_y),
                    &paint,
                );
            }
        }

        canvas.restore();

        // Scrollbar
        if total_h > visible_h {
            let max_scroll = total_h - visible_h;
            let thumb_h = (visible_h * visible_h / total_h).max(20.0);
            let thumb_y = list.top + (self.scroll_y / max_scroll) * (visible_h - thumb_h);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            paint.set_style(PaintStyle::Fill);
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
        _plan: &Plan,
    ) -> FloatingWindowOutcome {
        let prev_back = self.hovered_back;
        let prev_row = self.hovered_row;

        let back = Self::back_btn_rect(width, height);
        self.hovered_back = back.contains(skia_safe::Point::new(x, y));

        let list = Self::list_rect(width, height);
        self.hovered_row = if list.contains(skia_safe::Point::new(x, y)) {
            let rel_y = y - list.top + self.scroll_y;
            let row = (rel_y / ROW_H) as usize;
            let items = self.filter_results.borrow();
            if row < items.len() { Some(row) } else { None }
        } else {
            None
        };

        if self.hovered_back != prev_back || self.hovered_row != prev_row {
            FloatingWindowOutcome::dirty(DirtyRegion::All)
        } else {
            FloatingWindowOutcome::default()
        }
    }

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

        let back = Self::back_btn_rect(width, height);
        if back.contains(skia_safe::Point::new(x, y)) {
            return FloatingWindowOutcome::close();
        }

        let list = Self::list_rect(width, height);
        if list.contains(skia_safe::Point::new(x, y)) {
            let rel_y = y - list.top + self.scroll_y;
            let row = (rel_y / ROW_H) as usize;
            let items = self.filter_results.borrow();
            if let Some((node_id, _, is_milestone)) = items.get(row) {
                let node_id = *node_id;
                let is_milestone = *is_milestone;
                drop(items);
                // Open edit form as a child window; keep search open underneath.
                let edit_win: Box<dyn FloatingWindow> = match node_id {
                    NodeId::Task(task_id) => {
                        if let Some(task) = plan.tasks.get(&task_id) {
                            Box::new(TaskFormWindow::from_task(task, plan))
                        } else {
                            return FloatingWindowOutcome::default();
                        }
                    }
                    NodeId::Milestone(ms_id) => {
                        if is_milestone {
                            if let Some(ms) = plan.milestones.get(&ms_id) {
                                Box::new(MilestoneFormWindow::from_milestone(ms, plan))
                            } else {
                                return FloatingWindowOutcome::default();
                            }
                        } else {
                            return FloatingWindowOutcome::default();
                        }
                    }
                    NodeId::PlanStart => return FloatingWindowOutcome::default(),
                };
                self.pending_edit = Some(edit_win);
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
        }

        FloatingWindowOutcome::default()
    }

    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        self.pending_edit.take()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        modifiers: &Modifiers,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => {
                // Open edit form for the first result.
                let items = Self::compute_items(plan, &self.filter.content);
                if let Some((node_id, _, is_milestone)) = items.first() {
                    let edit_win: Option<Box<dyn FloatingWindow>> = match node_id {
                        NodeId::Task(task_id) => {
                            plan.tasks.get(task_id).map(|t| -> Box<dyn FloatingWindow> {
                                Box::new(TaskFormWindow::from_task(t, plan))
                            })
                        }
                        NodeId::Milestone(ms_id) if *is_milestone => plan
                            .milestones
                            .get(ms_id)
                            .map(|m| -> Box<dyn FloatingWindow> {
                                Box::new(MilestoneFormWindow::from_milestone(m, plan))
                            }),
                        _ => None,
                    };
                    if let Some(w) = edit_win {
                        self.pending_edit = Some(w);
                        return FloatingWindowOutcome::dirty(DirtyRegion::All);
                    }
                }
                FloatingWindowOutcome::default()
            }
            _ => {
                if self.filter.handle_key(key, modifiers) {
                    self.scroll_y = 0.0;
                    FloatingWindowOutcome::dirty(DirtyRegion::All)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
        }
    }

    fn on_paste(
        &mut self,
        text: &str,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        self.filter.handle_paste(text);
        self.scroll_y = 0.0;
        FloatingWindowOutcome::dirty(DirtyRegion::All)
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _plan: &Plan,
        _width: f32,
        _height: f32,
    ) -> FloatingWindowOutcome {
        let items = self.filter_results.borrow();
        let total_h = items.len() as f32 * ROW_H;
        // Use a fixed approximation for visible height (PANEL_H minus chrome).
        let visible_h = PANEL_H - TITLE_H - INPUT_PAD - INPUT_H - INPUT_PAD;
        let max_scroll = (total_h - visible_h).max(0.0);
        self.scroll_y = (self.scroll_y - delta_y * 3.0).clamp(0.0, max_scroll);
        FloatingWindowOutcome::dirty(DirtyRegion::All)
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_row = None;
    }
}
// }}}

// ── Local helpers ─────────────────────────────────────────────────────────────

fn draw_search_input(canvas: &Canvas, rect: Rect, input: &TextInput, cache: &RenderCache) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);

    // Background
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);

    // Border (always focused-style since search input is auto-focused)
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let h_pad = 8.0;
    let inner = Rect::from_xywh(
        rect.left + h_pad,
        rect.top + 2.0,
        rect.width() - 2.0 * h_pad,
        rect.height() - 4.0,
    );

    let cursor_pos = input.cursor.min(input.content.len());
    let cursor_x_px = if cursor_pos == 0 {
        0.0f32
    } else {
        cache.font.measure_str(&input.content[..cursor_pos], None).0
    };

    let inner_w = inner.width();
    let prev = input.scroll_x.get();
    let scroll_x = if cursor_x_px < prev {
        cursor_x_px
    } else if cursor_x_px > prev + inner_w {
        cursor_x_px - inner_w + 8.0
    } else {
        prev
    };
    input.scroll_x.set(scroll_x);

    canvas.save();
    canvas.clip_rect(inner, ClipOp::Intersect, false);
    let (_, metrics) = cache.font.metrics();
    let text_y =
        rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

    if input.content.is_empty() {
        if let Some(blob) = TextBlob::new("Search tasks and milestones…", &cache.font) {
            paint.set_color(Color::from(PLACEHOLDER_FG));
            canvas.draw_text_blob(&blob, (inner.left, text_y), &paint);
        }
    } else if let Some(blob) = TextBlob::new(&input.content, &cache.font) {
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (inner.left - scroll_x, text_y), &paint);
    }

    // Cursor
    paint.set_color(Color::from(INPUT_CURSOR_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(
            inner.left + cursor_x_px - scroll_x,
            rect.top + 5.0,
            1.5,
            rect.height() - 10.0,
        ),
        &paint,
    );

    canvas.restore();
}
