//! Floating window for managing the plan's ordered tag registry.

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_SIZE, BTN_PRIMARY_BG, BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG,
    DIVIDER_COLOR, ERROR_BG, ICON_DELETE_COLOR, INPUT_BG, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS,
    INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LINK_COLOR, LIST_BG, LIST_ITEM_HOVER_BG, OVERLAY_DARK,
    OVERLAY_MEDIUM, OVERLAY_SOFT, OVERLAY_XLIGHT, PANEL_BG, PANEL_TEXT, PLAN_BTN_CORNER,
    PLAN_LIST_ITEM_H, SCROLLBAR_THUMB_COLOR, TOOLBAR_BTN_HOVER_BG, TOOLBAR_BTN_ICON_COLOR,
    TOOLBAR_STROKE_WIDTH, TOOLTIP_BG,
};
use crate::ui::text_input::TextInput;
use plinko_shared::data::{Plan, TagId};
use plinko_shared::protocol::PlanRequest;

const PANEL_W: f32 = 420.0;
const PANEL_H: f32 = 520.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const ROW_H: f32 = PLAN_LIST_ITEM_H;
const PADDING: f32 = 16.0;
const HANDLE_W: f32 = 32.0;
const DELETE_W: f32 = 28.0;
const SCROLLBAR_W: f32 = 4.0;
/// Height of the inline add-tag footer (shown when add_input is Some).
const FOOTER_H: f32 = 52.0;
/// Width of the Save/confirm button in the footer.
const CONFIRM_BTN_W: f32 = 64.0;

pub struct TagsWindow {
    scroll_offset: f32,
    hovered_back: bool,
    hovered_plus: bool,
    hovered_row: Option<usize>,
    hovered_delete: Option<usize>,
    /// (src_tag_index, current_mouse_y) while a drag is in progress.
    drag_state: Option<(usize, f32)>,
    /// When Some, the add-tag footer is visible with an inline text input.
    add_input: Option<TextInput>,
    hovered_confirm: bool,
    add_input_error: bool,
    rename_error: bool,
    /// When Some, the tag with this `TagId` is being renamed via the inline input.
    rename_state: Option<(TagId, TextInput)>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TagsWindow {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            hovered_back: false,
            hovered_plus: false,
            hovered_row: None,
            hovered_delete: None,
            drag_state: None,
            add_input: None,
            hovered_confirm: false,
            add_input_error: false,
            rename_error: false,
            rename_state: None,
        }
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph)
    }

    fn back_btn_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(
            p.left + BTN_INSET,
            p.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn plus_btn_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(
            p.right - BTN_INSET - BACK_BTN_SIZE,
            p.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn list_rect(width: f32, height: f32, has_footer: bool) -> Rect {
        let p = Self::panel_rect(width, height);
        let footer = if has_footer { FOOTER_H } else { 0.0 };
        Rect::from_xywh(
            p.left,
            p.top + TITLE_H + 1.0,
            p.width(),
            p.height() - TITLE_H - 1.0 - footer,
        )
    }

    fn footer_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(p.left, p.bottom - FOOTER_H, p.width(), FOOTER_H)
    }

    fn confirm_btn_rect(width: f32, height: f32) -> Rect {
        let f = Self::footer_rect(width, height);
        Rect::from_xywh(
            f.right - PADDING - CONFIRM_BTN_W,
            f.top + (FOOTER_H - 28.0) / 2.0,
            CONFIRM_BTN_W,
            28.0,
        )
    }

    fn input_rect(width: f32, height: f32) -> Rect {
        let f = Self::footer_rect(width, height);
        let btn = Self::confirm_btn_rect(width, height);
        Rect::from_xywh(
            f.left + PADDING,
            f.top + (FOOTER_H - 28.0) / 2.0,
            btn.left - f.left - PADDING * 2.0,
            28.0,
        )
    }

    fn row_y(&self, list_top: f32, idx: usize) -> f32 {
        list_top + idx as f32 * ROW_H - self.scroll_offset
    }

    fn max_scroll(n_tags: usize, width: f32, height: f32, has_footer: bool) -> f32 {
        let content_h = n_tags as f32 * ROW_H;
        let list_h = Self::list_rect(width, height, has_footer).height();
        (content_h - list_h).max(0.0)
    }

    fn hovered_row_for(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        n_tags: usize,
    ) -> Option<usize> {
        let list = Self::list_rect(width, height, self.add_input.is_some());
        if !list.contains(Point::new(x, y)) {
            return None;
        }
        let rel_y = y - list.top + self.scroll_offset;
        let idx = (rel_y / ROW_H) as usize;
        if idx < n_tags { Some(idx) } else { None }
    }

    /// Compute the insertion gap index (0..=n_tags) from a drag Y position.
    fn drag_gap(mouse_y: f32, list_top: f32, scroll_offset: f32, n_tags: usize) -> usize {
        let rel_y = mouse_y - list_top + scroll_offset;
        let gap = ((rel_y + ROW_H / 2.0) / ROW_H) as i32;
        gap.clamp(0, n_tags as i32) as usize
    }

    /// Convert a visual gap index to the `new_index` argument for `MoveTag`.
    fn gap_to_new_index(gap: usize, src_idx: usize) -> usize {
        if gap <= src_idx { gap } else { gap - 1 }
    }

    fn draw_plus_btn(canvas: &Canvas, btn: Rect, hovered: bool, icon: &skia_safe::Path) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        if hovered {
            paint.set_color(Color::from(TOOLBAR_BTN_HOVER_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(btn, BACK_BTN_CORNER, BACK_BTN_CORNER),
                &paint,
            );
        }
        let draw_size = BACK_BTN_SIZE * 0.5;
        let ox = btn.left + (BACK_BTN_SIZE - draw_size) / 2.0;
        let oy = btn.top + (BACK_BTN_SIZE - draw_size) / 2.0;
        let bounds = icon.bounds();
        let sx = draw_size / bounds.width().max(1.0);
        let sy = draw_size / bounds.height().max(1.0);
        let matrix = skia_safe::Matrix::scale_translate(
            (sx, sy),
            (ox - bounds.left * sx, oy - bounds.top * sy),
        );
        let scaled = icon.with_transform(&matrix);
        paint.set_color(Color::from(TOOLBAR_BTN_ICON_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
        canvas.draw_path(&scaled, &paint);
    }

    /// Draw a grip handle (3×2 dots) centred in the given zone rect.
    fn draw_grip(canvas: &Canvas, zone: Rect) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from(OVERLAY_MEDIUM));
        paint.set_style(PaintStyle::Fill);
        let r = 2.0;
        let col_gap = 5.0;
        let row_gap = 5.0;
        let cols = 2;
        let rows = 3;
        let grid_w = cols as f32 * (2.0 * r) + (cols - 1) as f32 * col_gap;
        let grid_h = rows as f32 * (2.0 * r) + (rows - 1) as f32 * row_gap;
        let x0 = zone.left + (zone.width() - grid_w) / 2.0;
        let y0 = zone.top + (zone.height() - grid_h) / 2.0;
        for row in 0..rows {
            for col in 0..cols {
                let cx = x0 + col as f32 * (2.0 * r + col_gap) + r;
                let cy = y0 + row as f32 * (2.0 * r + row_gap) + r;
                canvas.draw_circle((cx, cy), r, &paint);
            }
        }
    }

    /// Draw a small × delete icon centred in the given zone rect.
    fn draw_delete_icon(canvas: &Canvas, zone: Rect, hovered: bool) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        if hovered {
            paint.set_color(Color::from(ERROR_BG));
            paint.set_style(PaintStyle::Fill);
            let r = zone.width().min(zone.height()) / 2.0 - 2.0;
            let cx = zone.left + zone.width() / 2.0;
            let cy = zone.top + zone.height() / 2.0;
            canvas.draw_circle((cx, cy), r, &paint);
        }
        let s = 6.0; // half-size of the × arms
        let cx = zone.left + zone.width() / 2.0;
        let cy = zone.top + zone.height() / 2.0;
        let mut pb = PathBuilder::new();
        pb.move_to((cx - s, cy - s));
        pb.line_to((cx + s, cy + s));
        pb.move_to((cx + s, cy - s));
        pb.line_to((cx - s, cy + s));
        paint.set_color(if hovered {
            Color::from(ICON_DELETE_COLOR)
        } else {
            Color::from(OVERLAY_DARK)
        });
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_path(&pb.detach(), &paint);
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl FloatingWindow for TagsWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let has_footer = self.add_input.is_some();
        let list = Self::list_rect(width, height, has_footer);
        let back_btn = Self::back_btn_rect(width, height);
        let plus_btn = Self::plus_btn_rect(width, height);

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

        // Title bar
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

        // Title text
        if let Some(blob) = TextBlob::new("Tags", &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str("Tags", None);
            let tx = panel.left + (panel.width() - advance) / 2.0;
            let ty =
                panel.top + (TITLE_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        crate::ui::window_chrome::draw_chevron_btn(canvas, back_btn, self.hovered_back);
        Self::draw_plus_btn(canvas, plus_btn, self.hovered_plus, &cache.icon_plus);

        // Divider below title
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // ── Tag list ──────────────────────────────────────────────────────────
        canvas.save();
        canvas.clip_rect(list, ClipOp::Intersect, false);

        if plan.tags.is_empty() {
            if let Some(blob) = TextBlob::new("No tags yet", &cache.font) {
                let (_, metrics) = cache.font.metrics();
                let (advance, _) = cache.font.measure_str("No tags yet", None);
                let tx = panel.left + (panel.width() - advance) / 2.0;
                let ty = list.top + 48.0 - metrics.ascent;
                paint.set_color(Color::from(PANEL_TEXT));
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
        } else {
            let (_, metrics) = cache.font.metrics();
            let text_y_offset = (ROW_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

            // Compute drag gap for insertion indicator
            let drag_gap = self.drag_state.map(|(src, dy)| {
                let gap = Self::drag_gap(dy, list.top, self.scroll_offset, plan.tags.len());
                (src, gap)
            });

            for (i, tag) in plan.tags.iter().enumerate() {
                let ry = self.row_y(list.top, i);
                if ry + ROW_H < list.top || ry > list.bottom {
                    continue;
                }

                let is_dragged = self.drag_state.map(|(s, _)| s == i).unwrap_or(false);
                let is_renaming = self
                    .rename_state
                    .as_ref()
                    .map(|(id, _)| id == &tag.id)
                    .unwrap_or(false);

                // Row hover background
                let is_hovered = self.hovered_row == Some(i) && !is_dragged;
                if is_hovered {
                    paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_rect(
                        Rect::from_xywh(panel.left, ry, panel.width(), ROW_H),
                        &paint,
                    );
                }

                // Alpha for dragged row
                let _alpha = if is_dragged { 60u8 } else { 255u8 };

                // Grip handle zone
                let handle_zone = Rect::from_xywh(panel.left, ry, PADDING + HANDLE_W, ROW_H);
                // Only draw grip if not dragged (dragged row is ghost elsewhere)
                if !is_dragged {
                    Self::draw_grip(canvas, handle_zone);
                }

                if is_renaming {
                    // Draw inline rename input in the tag name area
                    if let Some((_, ref input)) = self.rename_state {
                        let input_rect = Rect::from_xywh(
                            panel.left + PADDING + HANDLE_W + 4.0,
                            ry + 4.0,
                            panel.right
                                - PADDING
                                - DELETE_W
                                - SCROLLBAR_W
                                - 2.0
                                - (panel.left + PADDING + HANDLE_W + 4.0)
                                - 4.0,
                            ROW_H - 8.0,
                        );
                        // Background
                        paint.set_color(Color::from(INPUT_BG));
                        paint.set_style(PaintStyle::Fill);
                        canvas.draw_rrect(
                            RRect::new_rect_xy(input_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                            &paint,
                        );
                        // Border
                        paint.set_color(Color::from(if self.rename_error {
                            INPUT_BORDER_ERROR
                        } else {
                            INPUT_BORDER_FOCUS
                        }));
                        paint.set_style(PaintStyle::Stroke);
                        paint.set_stroke_width(if self.rename_error { 2.0 } else { 1.0 });
                        canvas.draw_rrect(
                            RRect::new_rect_xy(input_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                            &paint,
                        );
                        paint.set_style(PaintStyle::Fill);
                        // Text + cursor
                        let h_pad = 6.0;
                        let inner = Rect::from_xywh(
                            input_rect.left + h_pad,
                            input_rect.top + 2.0,
                            input_rect.width() - 2.0 * h_pad,
                            input_rect.height() - 4.0,
                        );
                        canvas.save();
                        canvas.clip_rect(inner, ClipOp::Intersect, false);
                        let (_, fm) = cache.font.metrics();
                        let text_y = input_rect.top
                            + (input_rect.height() - (fm.descent - fm.ascent)) / 2.0
                            - fm.ascent;
                        if !input.content.is_empty()
                            && let Some(blob) = TextBlob::new(&input.content, &cache.font)
                        {
                            paint.set_color(Color::from(INPUT_FG));
                            canvas.draw_text_blob(&blob, (inner.left, text_y), &paint);
                        }
                        let cursor_x = if input.cursor == 0 {
                            0.0
                        } else {
                            let (adv, _) = cache.font.measure_str(
                                &input.content[..input.cursor.min(input.content.len())],
                                None,
                            );
                            adv
                        };
                        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
                        canvas.draw_rect(
                            Rect::from_xywh(
                                inner.left + cursor_x,
                                input_rect.top + 3.0,
                                1.5,
                                input_rect.height() - 6.0,
                            ),
                            &paint,
                        );
                        canvas.restore();
                    }
                } else {
                    // Tag name
                    if let Some(blob) = TextBlob::new(&tag.name, &cache.font) {
                        paint.set_color(Color::from(ITEM_FG));
                        paint.set_style(PaintStyle::Fill);
                        let tx = panel.left + PADDING + HANDLE_W + 4.0;
                        canvas.draw_text_blob(&blob, (tx, ry + text_y_offset), &paint);
                    }
                }

                // Delete button zone
                let del_zone = Rect::from_xywh(
                    panel.right - PADDING - DELETE_W - SCROLLBAR_W - 2.0,
                    ry,
                    DELETE_W,
                    ROW_H,
                );
                if !is_dragged {
                    Self::draw_delete_icon(canvas, del_zone, self.hovered_delete == Some(i));
                }

                // Row divider
                if i + 1 < plan.tags.len() && !is_dragged {
                    paint.set_color(Color::from(DIVIDER_COLOR));
                    canvas.draw_rect(
                        Rect::from_xywh(
                            panel.left + PADDING + HANDLE_W,
                            ry + ROW_H - 1.0,
                            panel.width() - 2.0 * PADDING - HANDLE_W,
                            1.0,
                        ),
                        &paint,
                    );
                }
            }

            // Insertion indicator line
            if let Some((_, gap)) = drag_gap {
                let line_y = list.top + gap as f32 * ROW_H - self.scroll_offset;
                let line_y = line_y.clamp(list.top, list.bottom);
                paint.set_color(Color::from(LINK_COLOR));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rect(
                    Rect::from_xywh(
                        panel.left + PADDING,
                        line_y - 1.5,
                        panel.width() - 2.0 * PADDING,
                        3.0,
                    ),
                    &paint,
                );
            }

            // Ghost row (dragged tag following mouse)
            if let Some((src_idx, drag_y)) = self.drag_state
                && let Some(tag) = plan.tags.get(src_idx)
            {
                let gy = drag_y - ROW_H / 2.0;
                // Ghost background
                paint.set_color(Color::from(TOOLTIP_BG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rect(
                    Rect::from_xywh(panel.left, gy, panel.width(), ROW_H),
                    &paint,
                );
                // Ghost shadow
                paint.set_color(Color::from(OVERLAY_XLIGHT));
                canvas.draw_rect(
                    Rect::from_xywh(panel.left, gy + ROW_H, panel.width(), 3.0),
                    &paint,
                );
                // Ghost grip
                Self::draw_grip(
                    canvas,
                    Rect::from_xywh(panel.left, gy, PADDING + HANDLE_W, ROW_H),
                );
                // Ghost name
                if let Some(blob) = TextBlob::new(&tag.name, &cache.font) {
                    paint.set_color(Color::from(ITEM_FG));
                    canvas.draw_text_blob(
                        &blob,
                        (panel.left + PADDING + HANDLE_W + 4.0, gy + text_y_offset),
                        &paint,
                    );
                }
            }
        }

        canvas.restore();

        // Scrollbar
        let n = plan.tags.len();
        let max_scroll = Self::max_scroll(n, width, height, has_footer);
        if max_scroll > 0.0 {
            let list_h = list.height();
            let content_h = n as f32 * ROW_H;
            let thumb_h = (list_h * list_h / content_h).max(20.0);
            let thumb_y = list.top + (self.scroll_offset / max_scroll) * (list_h - thumb_h);
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

        // ── Footer (add input) ────────────────────────────────────────────────
        if let Some(input) = &self.add_input {
            let footer = Self::footer_rect(width, height);
            let input_rect = Self::input_rect(width, height);
            let confirm_btn = Self::confirm_btn_rect(width, height);

            // Footer divider
            paint.set_color(Color::from(DIVIDER_COLOR));
            canvas.draw_rect(
                Rect::from_xywh(panel.left, footer.top, panel.width(), 1.0),
                &paint,
            );

            // Input field background + border
            let rrect = RRect::new_rect_xy(input_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
            paint.set_color(Color::from(INPUT_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(rrect, &paint);
            paint.set_color(Color::from(if self.add_input_error {
                INPUT_BORDER_ERROR
            } else {
                INPUT_BORDER_FOCUS
            }));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(if self.add_input_error { 2.0 } else { 1.0 });
            canvas.draw_rrect(rrect, &paint);

            // Input text + cursor
            let h_pad = 8.0;
            let inner = Rect::from_xywh(
                input_rect.left + h_pad,
                input_rect.top + 2.0,
                input_rect.width() - 2.0 * h_pad,
                input_rect.height() - 4.0,
            );
            canvas.save();
            canvas.clip_rect(inner, ClipOp::Intersect, false);
            let (_, fm) = cache.font.metrics();
            let text_y =
                input_rect.top + (input_rect.height() - (fm.descent - fm.ascent)) / 2.0 - fm.ascent;
            if !input.content.is_empty()
                && let Some(blob) = TextBlob::new(&input.content, &cache.font)
            {
                paint.set_color(Color::from(INPUT_FG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_text_blob(&blob, (inner.left, text_y), &paint);
            }
            let cursor_x = if input.cursor == 0 {
                0.0
            } else {
                let (adv, _) = cache.font.measure_str(
                    &input.content[..input.cursor.min(input.content.len())],
                    None,
                );
                adv
            };
            paint.set_color(Color::from(INPUT_CURSOR_COLOR));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(
                Rect::from_xywh(
                    inner.left + cursor_x,
                    input_rect.top + 5.0,
                    1.5,
                    input_rect.height() - 10.0,
                ),
                &paint,
            );
            canvas.restore();

            // Confirm button
            paint.set_color(Color::from(if self.hovered_confirm {
                BTN_PRIMARY_HOVER_BG
            } else {
                BTN_PRIMARY_BG
            }));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(confirm_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            if let Some(blob) = TextBlob::new("Add", &cache.font) {
                let (_, fm) = cache.font.metrics();
                let (advance, _) = cache.font.measure_str("Add", None);
                let tx = confirm_btn.left + (CONFIRM_BTN_W - advance) / 2.0;
                let ty = confirm_btn.top + (confirm_btn.height() - (fm.descent - fm.ascent)) / 2.0
                    - fm.ascent;
                paint.set_color(Color::from(BTN_PRIMARY_FG));
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
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
        // Update drag position
        if let Some((_, ref mut dy)) = self.drag_state {
            *dy = y;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        let pt = Point::new(x, y);
        let new_back = Self::back_btn_rect(width, height).contains(pt);
        let new_plus = Self::plus_btn_rect(width, height).contains(pt);
        let new_confirm =
            self.add_input.is_some() && Self::confirm_btn_rect(width, height).contains(pt);
        let n = plan.tags.len();
        let new_row = self.hovered_row_for(x, y, width, height, n);

        // Delete button hover: only if on a row and x is in delete zone
        let new_delete = new_row.and_then(|idx| {
            let panel = Self::panel_rect(width, height);
            let del_x = panel.right - PADDING - DELETE_W - SCROLLBAR_W - 2.0;
            if x >= del_x && x <= del_x + DELETE_W {
                Some(idx)
            } else {
                None
            }
        });

        if new_back != self.hovered_back
            || new_plus != self.hovered_plus
            || new_confirm != self.hovered_confirm
            || new_row != self.hovered_row
            || new_delete != self.hovered_delete
        {
            self.hovered_back = new_back;
            self.hovered_plus = new_plus;
            self.hovered_confirm = new_confirm;
            self.hovered_row = new_row;
            self.hovered_delete = new_delete;
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
        sender: &PlanRequestSender,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        // Handle drag release
        if !pressed {
            if let Some((src_idx, drag_y)) = self.drag_state.take() {
                let list = Self::list_rect(width, height, self.add_input.is_some());
                let n = plan.tags.len();
                if n > 1 {
                    let gap = Self::drag_gap(drag_y, list.top, self.scroll_offset, n);
                    let new_index = Self::gap_to_new_index(gap, src_idx);
                    if new_index != src_idx
                        && let Some(tag) = plan.tags.get(src_idx)
                    {
                        sender.send(PlanRequest::MoveTag(tag.id, new_index));
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }

        let pt = Point::new(x, y);

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::plus_btn_rect(width, height).contains(pt) {
            if self.add_input.is_none() {
                let mut inp = TextInput::new("");
                inp.focused = true;
                self.add_input = Some(inp);
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Footer interactions
        if self.add_input.is_some() {
            if Self::confirm_btn_rect(width, height).contains(pt) {
                return self.submit_add(sender);
            }
            if Self::input_rect(width, height).contains(pt) {
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        if !Self::panel_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }

        // Row interactions
        if let Some(idx) = self.hovered_row_for(x, y, width, height, plan.tags.len()) {
            let panel = Self::panel_rect(width, height);

            // Delete button zone
            let del_x = panel.right - PADDING - DELETE_W - SCROLLBAR_W - 2.0;
            if x >= del_x && x <= del_x + DELETE_W {
                if let Some(tag) = plan.tags.get(idx) {
                    sender.send(PlanRequest::DeleteTag(tag.id));
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }

            // Handle zone — start drag
            let handle_end_x = panel.left + PADDING + HANDLE_W;
            if x <= handle_end_x {
                self.drag_state = Some((idx, y));
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }

            // Tag name zone — start rename
            let name_end_x = panel.right - PADDING - DELETE_W - SCROLLBAR_W - 2.0;
            if x > handle_end_x && x < name_end_x {
                if let Some(tag) = plan.tags.get(idx) {
                    let tag_id = tag.id;
                    let current_name = tag.name.clone();
                    self.rename_state = Some((tag_id, TextInput::new(&current_name)));
                    self.rename_error = false;
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        modifiers: &Modifiers,
        sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        // Rename state takes priority
        if self.rename_state.is_some() {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.rename_state = None;
                    self.rename_error = false;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some((tag_id, ref input)) = self.rename_state {
                        let new_name = input.content.trim().to_string();
                        if new_name.is_empty() {
                            self.rename_error = true;
                            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                        }
                        sender.send(PlanRequest::RenameTag(tag_id, new_name));
                    }
                    self.rename_state = None;
                    self.rename_error = false;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                _ => {}
            }
            if let Some((_, ref mut input)) = self.rename_state
                && input.handle_key(key, modifiers)
            {
                self.rename_error = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }

        if let Some(input) = &mut self.add_input {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.add_input = None;
                    self.add_input_error = false;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => return self.submit_add(sender),
                _ => {}
            }
            if input.handle_key(key, modifiers) {
                self.add_input_error = false;
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }

        if *key == Key::Named(NamedKey::Escape) {
            FloatingWindowOutcome::close()
        } else {
            FloatingWindowOutcome::default()
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
        if let Some((_, ref mut input)) = self.rename_state {
            input.handle_paste(text);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if let Some(ref mut input) = self.add_input {
            input.handle_paste(text);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
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
        let max = Self::max_scroll(plan.tags.len(), width, height, self.add_input.is_some());
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

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_plus = false;
        self.hovered_row = None;
        self.hovered_delete = None;
        self.hovered_confirm = false;
        self.rename_state = None;
    }
}
// }}}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TagsWindow {
    fn submit_add(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self
            .add_input
            .as_ref()
            .map(|i| i.content.trim().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            self.add_input_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        sender.send(PlanRequest::AddTag(name));
        self.add_input = None;
        self.add_input_error = false;
        FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
    }
}
// }}}
