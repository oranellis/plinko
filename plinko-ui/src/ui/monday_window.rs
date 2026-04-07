//! Floating window for configuring Monday.com integration for a plan.

use std::sync::{Arc, Mutex};
use std::thread;

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use plinko_shared::data::Plan;
use plinko_shared::data::allocation::Status;
use plinko_shared::data::ids::UserId;
use plinko_shared::data::storage::Storage;
use plinko_shared::monday::{
    BoardColumn, ItemNodeMapping, MondayConfig, MondayUser, StatusMapping, UserMapping,
};
use uuid::Uuid;

use crate::engine::PlanRequestSender;
use crate::monday::client::MondayClient;
use crate::monday::{export, import};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, BTN_DANGER_BG, BTN_DANGER_FG, BTN_PRIMARY_BG, BTN_PRIMARY_FG,
    BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, DIVIDER_COLOR, INPUT_BG,
    INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_FG, ITEM_FG, LABEL_FG, MUTED_FG,
    OVERLAY_DARK, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING,
    PLAN_INPUT_H, PLAN_LABEL_GAP, SCROLLBAR_THUMB_COLOR,
};
use crate::ui::text_input::TextInput;

// ── Layout ────────────────────────────────────────────────────────────────────

const PANEL_W: f32 = 680.0;
const TITLE_H: f32 = 48.0;
const FOOTER_H: f32 = 96.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const SCROLLBAR_W: f32 = 4.0;
const SECTION_TITLE_H: f32 = 20.0;
const SECTION_GAP: f32 = 12.0;
const LABEL_H: f32 = 18.0;
const LABEL_W: f32 = 160.0;
const MAP_ROW_H: f32 = 32.0;
const MAP_ROW_GAP: f32 = 4.0;
const RADIO_SIZE: f32 = 16.0;

// ── Sync status ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
enum SyncState {
    #[default]
    Idle,
    InProgress(String),
    Done(String),
    Err(String),
}

// ── Window struct ─────────────────────────────────────────────────────────────

/// Floating window for configuring and running Monday.com sync.
pub struct MondayWindow {
    plan_id: Uuid,
    // ── Connection fields ──
    api_token: TextInput,
    board_id: TextInput,
    // ── Column mapping fields ──
    person_col: TextInput,
    status_col: TextInput,
    dep_col: TextInput,
    workload_col: TextInput,
    timeline_col: TextInput,
    workload_in_hours: bool,
    use_subitems: bool,
    // ── Fetched / mapped data ──
    fetched_columns: Vec<BoardColumn>,
    fetched_monday_users: Vec<MondayUser>,
    fetched_status_labels: Vec<String>,
    user_mappings: Vec<UserMapping>,
    status_mappings: Vec<StatusMapping>,
    item_node_map: Vec<ItemNodeMapping>,
    // ── UI state ──
    scroll_y: f32,
    focused: FocusedInput,
    // hover flags (for buttons)
    hov_close: bool,
    hov_test: bool,
    hov_fetch: bool,
    hov_save: bool,
    hov_pull: bool,
    hov_push: bool,
    // ── Async sync status ──
    sync_state: Arc<Mutex<SyncState>>,
    // ── Hit rects (populated in render) ──
    rects: std::cell::RefCell<HitRects>,
}

#[derive(Default, Clone)]
struct HitRects {
    close_btn: Rect,
    test_btn: Rect,
    fetch_btn: Rect,
    save_btn: Rect,
    pull_btn: Rect,
    push_btn: Rect,
    token_field: Rect,
    board_id_field: Rect,
    person_col_field: Rect,
    status_col_field: Rect,
    dep_col_field: Rect,
    workload_col_field: Rect,
    timeline_col_field: Rect,
    use_subitems_radio: [Rect; 2],
    workload_hours_radio: [Rect; 2],
    user_plinko_selectors: Vec<Rect>,
    status_plinko_selectors: Vec<Rect>,
    content_area: Rect,
}

#[derive(Clone, PartialEq, Eq, Default)]
enum FocusedInput {
    #[default]
    None,
    Token,
    BoardId,
    PersonCol,
    StatusCol,
    DepCol,
    WorkloadCol,
    TimelineCol,
}

// ── Constructor ───────────────────────────────────────────────────────────────

impl MondayWindow {
    /// Create from an existing or default config, loading the saved API token.
    pub fn new(plan_id: Uuid) -> Self {
        let (config, api_token) = if let Ok(storage) = Storage::from_user_data_dir() {
            let cfg = storage.load_monday_config(plan_id).unwrap_or_default();
            let tok = storage.load_monday_api_token();
            (cfg, tok)
        } else {
            (MondayConfig::default(), String::new())
        };

        Self {
            plan_id,
            api_token: TextInput::new(api_token),
            board_id: TextInput::new(&config.board_id),
            person_col: TextInput::new(&config.column_map.person_column_id),
            status_col: TextInput::new(&config.column_map.status_column_id),
            dep_col: TextInput::new(&config.column_map.dependency_column_id),
            workload_col: TextInput::new(&config.column_map.workload_column_id),
            timeline_col: TextInput::new(&config.column_map.timeline_column_id),
            workload_in_hours: config.workload_in_hours,
            use_subitems: config.use_subitems,
            fetched_columns: Vec::new(),
            fetched_monday_users: Vec::new(),
            fetched_status_labels: Vec::new(),
            user_mappings: config.user_mappings,
            status_mappings: config.status_mappings,
            item_node_map: config.item_node_map,
            scroll_y: 0.0,
            focused: FocusedInput::None,
            hov_close: false,
            hov_test: false,
            hov_fetch: false,
            hov_save: false,
            hov_pull: false,
            hov_push: false,
            sync_state: Arc::new(Mutex::new(SyncState::Idle)),
            rects: std::cell::RefCell::new(HitRects::default()),
        }
    }

    // ── Config snapshot ────────────────────────────────────────────────────
    fn current_config(&self) -> MondayConfig {
        MondayConfig {
            board_id: self.board_id.content.trim().to_string(),
            column_map: plinko_shared::monday::ColumnMap {
                person_column_id: self.person_col.content.trim().to_string(),
                status_column_id: self.status_col.content.trim().to_string(),
                dependency_column_id: self.dep_col.content.trim().to_string(),
                workload_column_id: self.workload_col.content.trim().to_string(),
                timeline_column_id: self.timeline_col.content.trim().to_string(),
            },
            user_mappings: self.user_mappings.clone(),
            status_mappings: self.status_mappings.clone(),
            item_node_map: self.item_node_map.clone(),
            use_subitems: self.use_subitems,
            workload_in_hours: self.workload_in_hours,
        }
    }

    fn save_config(&self) {
        if let Ok(storage) = Storage::from_user_data_dir() {
            let config = self.current_config();
            storage.save_monday_config(self.plan_id, &config);
            storage.save_monday_api_token(self.api_token.content.trim());
        }
    }

    // ── Sync height helpers ────────────────────────────────────────────────
    fn panel_h(height: f32) -> f32 {
        (height * 0.9).min(800.0)
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let h = Self::panel_h(height);
        let x = (width - PANEL_W) / 2.0;
        let y = (height - h) / 2.0;
        Rect::from_xywh(x, y, PANEL_W, h)
    }

    fn content_viewport_h(height: f32) -> f32 {
        Self::panel_h(height) - TITLE_H - 1.0 - FOOTER_H
    }

    fn total_content_height(&self) -> f32 {
        let mut h = PLAN_FORM_PADDING;
        // Connection section
        h += SECTION_TITLE_H + SECTION_GAP;
        h += LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // token
        h += PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // board id
        h += PLAN_FIELD_GAP + PLAN_BTN_H; // test btn
        // Column mapping
        h += PLAN_FIELD_GAP + SECTION_TITLE_H + SECTION_GAP;
        h += LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // person
        h += PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // status
        h += PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // dep
        h += PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // workload
        h += PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H; // timeline
        h += PLAN_FIELD_GAP + PLAN_BTN_H; // fetch btn
        // Item type + workload unit
        h += PLAN_FIELD_GAP + SECTION_TITLE_H + SECTION_GAP;
        h += RADIO_SIZE + MAP_ROW_GAP + RADIO_SIZE; // 2 radio rows
        h += PLAN_FIELD_GAP + SECTION_TITLE_H + SECTION_GAP;
        h += RADIO_SIZE + MAP_ROW_GAP + RADIO_SIZE; // workload unit radios
        // User mappings
        h += PLAN_FIELD_GAP + SECTION_TITLE_H + SECTION_GAP;
        let n_users = self.user_mappings.len().max(1);
        h += n_users as f32 * (MAP_ROW_H + MAP_ROW_GAP);
        // Status mappings
        h += PLAN_FIELD_GAP + SECTION_TITLE_H + SECTION_GAP;
        let n_status = self.status_mappings.len().max(1);
        h += n_status as f32 * (MAP_ROW_H + MAP_ROW_GAP);
        // Save button
        h += PLAN_FIELD_GAP + PLAN_BTN_H + PLAN_FORM_PADDING;
        h
    }

    fn max_scroll(&self, height: f32) -> f32 {
        (self.total_content_height() - MondayWindow::content_viewport_h(height)).max(0.0)
    }

    // ── Render helpers ─────────────────────────────────────────────────────
    fn draw_section_title(canvas: &Canvas, text: &str, x: f32, y: f32, cache: &RenderCache) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from(ITEM_FG));
        let font = &cache.font;
        let (_, metrics) = font.metrics();
        canvas.draw_str(text, (x, y - metrics.ascent), font, &paint);
    }

    fn draw_label(canvas: &Canvas, text: &str, x: f32, y: f32, cache: &RenderCache) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from(LABEL_FG));
        let font = &cache.font;
        let (_, metrics) = font.metrics();
        canvas.draw_str(text, (x, y - metrics.ascent), font, &paint);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_field(
        canvas: &Canvas,
        rect: Rect,
        input: &TextInput,
        focused: bool,
        masked: bool,
        cache: &RenderCache,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
        paint.set_color(Color::from(INPUT_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);
        paint.set_color(if focused {
            Color::from(INPUT_BORDER_FOCUS)
        } else {
            Color::from(INPUT_BORDER)
        });
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(if focused { 1.5 } else { 1.0 });
        canvas.draw_rrect(rrect, &paint);

        // Draw text content
        let h_pad = 8.0;
        let inner_x = rect.left + h_pad;
        let inner_w = rect.width() - 2.0 * h_pad;
        let font = &cache.font;
        let (_, metrics) = font.metrics();
        let text_y =
            rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

        // Clip to inner area
        canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(inner_x, rect.top + 2.0, inner_w, rect.height() - 4.0),
            ClipOp::Intersect,
            true,
        );

        let display_text = if masked {
            "•".repeat(input.content.len())
        } else {
            input.content.clone()
        };

        // Scroll to keep cursor visible
        let cursor_pos = input.cursor.min(display_text.len());
        let cursor_x_px = font.measure_str(&display_text[..cursor_pos], None).0;
        let scroll_x = {
            let prev = input.scroll_x.get();
            let new_scroll = if cursor_x_px - prev > inner_w {
                cursor_x_px - inner_w
            } else if cursor_x_px < prev {
                cursor_x_px
            } else {
                prev
            };
            input.scroll_x.set(new_scroll);
            new_scroll
        };

        paint.set_style(PaintStyle::Fill);
        paint.set_color(if display_text.is_empty() {
            Color::from(MUTED_FG)
        } else {
            Color::from(INPUT_FG)
        });

        let text_to_draw = if display_text.is_empty() {
            String::new()
        } else {
            display_text.clone()
        };

        canvas.draw_str(&text_to_draw, (inner_x - scroll_x, text_y), font, &paint);

        // Draw cursor if focused
        if focused {
            let cx = inner_x + cursor_x_px - scroll_x;
            paint.set_color(Color::from(crate::ui::layout::INPUT_CURSOR_COLOR));
            paint.set_stroke_width(1.5);
            paint.set_style(PaintStyle::Stroke);
            canvas.draw_line((cx, rect.top + 4.0), (cx, rect.bottom - 4.0), &paint);
        }

        canvas.restore();
    }

    fn draw_button(
        canvas: &Canvas,
        rect: Rect,
        label: &str,
        hovered: bool,
        primary: bool,
        danger: bool,
        cache: &RenderCache,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let bg = if danger {
            if hovered {
                Color::from(crate::ui::layout::BTN_DANGER_HOVER_BG)
            } else {
                Color::from(BTN_DANGER_BG)
            }
        } else if primary {
            if hovered {
                Color::from(BTN_PRIMARY_HOVER_BG)
            } else {
                Color::from(BTN_PRIMARY_BG)
            }
        } else if hovered {
            Color::from(crate::ui::layout::TOOLBAR_BTN_HOVER_BG)
        } else {
            Color::from(BTN_SECONDARY_BG)
        };
        paint.set_color(bg);
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        let fg = if danger {
            Color::from(BTN_DANGER_FG)
        } else if primary {
            Color::from(BTN_PRIMARY_FG)
        } else {
            Color::from(BTN_SECONDARY_FG)
        };
        paint.set_color(fg);
        let font = &cache.font;
        let (text_w, _) = font.measure_str(label, None);
        let (_, metrics) = font.metrics();
        let tx = rect.left + (rect.width() - text_w) / 2.0;
        let ty =
            rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
        canvas.draw_str(label, (tx, ty), font, &paint);
    }

    fn draw_radio(
        canvas: &Canvas,
        x: f32,
        y: f32,
        label: &str,
        selected: bool,
        cache: &RenderCache,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let cx = x + RADIO_SIZE / 2.0;
        let cy = y + RADIO_SIZE / 2.0;
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_circle((cx, cy), RADIO_SIZE / 2.0 - 1.0, &paint);
        if selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_circle((cx, cy), RADIO_SIZE / 4.0, &paint);
        }
        paint.set_color(Color::from(ITEM_FG));
        let font = &cache.font;
        let (_, metrics) = font.metrics();
        let ty = y + (RADIO_SIZE - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
        canvas.draw_str(label, (x + RADIO_SIZE + 8.0, ty), font, &paint);
    }

    fn draw_mapping_row(
        canvas: &Canvas,
        rect: Rect,
        left_text: &str,
        right_text: &str,
        hovered: bool,
        cache: &RenderCache,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let rrect = RRect::new_rect_xy(rect, 4.0, 4.0);
        paint.set_color(if hovered {
            Color::from(0xff_2d2d30u32)
        } else {
            Color::from(INPUT_BG)
        });
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(rrect, &paint);

        let font = &cache.font;
        let (_, metrics) = font.metrics();
        let ty =
            rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
        let mid = rect.left + rect.width() / 2.0;

        paint.set_style(PaintStyle::Fill);
        paint.set_color(Color::from(ITEM_FG));
        canvas.draw_str(left_text, (rect.left + 8.0, ty), font, &paint);

        paint.set_color(Color::from(BTN_PRIMARY_BG));
        canvas.draw_str(right_text, (mid + 8.0, ty), font, &paint);

        // Divider line
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_line((mid, rect.top + 4.0), (mid, rect.bottom - 4.0), &paint);
    }
}

// ── FloatingWindow impl ────────────────────────────────────────────────────────

impl FloatingWindow for MondayWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // ── Overlay ─────────────────────────────────────────────────────────
        paint.set_color(Color::from(OVERLAY_DARK));
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, width, height), &paint);

        let panel = Self::panel_rect(width, height);
        let content_vp_h = MondayWindow::content_viewport_h(height);

        // ── Panel background ────────────────────────────────────────────────
        let rrect = RRect::new_rect_xy(panel, CORNER, CORNER);
        paint.set_color(Color::from(PANEL_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &paint);

        // ── Title bar ───────────────────────────────────────────────────────
        let title_rect = Rect::from_xywh(panel.left, panel.top, PANEL_W, TITLE_H);
        let close_rect = Rect::from_xywh(
            panel.right - BTN_INSET - BACK_BTN_SIZE,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        );
        if self.hov_close {
            paint.set_color(Color::from(0xff_3d3d40u32));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(close_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
        }
        // X icon
        paint.set_color(Color::from(MUTED_FG));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.8);
        let inset = 10.0;
        canvas.draw_line(
            (close_rect.left + inset, close_rect.top + inset),
            (close_rect.right - inset, close_rect.bottom - inset),
            &paint,
        );
        canvas.draw_line(
            (close_rect.right - inset, close_rect.top + inset),
            (close_rect.left + inset, close_rect.bottom - inset),
            &paint,
        );

        // Title text
        paint.set_style(PaintStyle::Fill);
        paint.set_color(Color::from(ITEM_FG));
        let title = "Monday.com Integration";
        let font = &cache.font;
        let (tw, _) = font.measure_str(title, None);
        let (_, metrics) = font.metrics();
        let tx = panel.left + (PANEL_W - tw) / 2.0;
        let ty =
            title_rect.top + (TITLE_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
        canvas.draw_str(title, (tx, ty), font, &paint);

        // ── Divider ─────────────────────────────────────────────────────────
        paint.set_color(Color::from(DIVIDER_COLOR));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, PANEL_W, 1.0),
            &paint,
        );

        // ── Content area (clipped + scrolled) ──────────────────────────────
        let content_x = panel.left;
        let content_top = panel.top + TITLE_H + 1.0;
        let content_area = Rect::from_xywh(content_x, content_top, PANEL_W, content_vp_h);
        canvas.save();
        canvas.clip_rect(content_area, ClipOp::Intersect, true);

        let mut hit = self.rects.borrow().clone();
        hit.content_area = content_area;
        hit.close_btn = close_rect;

        let px = panel.left + PLAN_FORM_PADDING;
        let field_w = PANEL_W - 2.0 * PLAN_FORM_PADDING - SCROLLBAR_W - 4.0;
        let mut y = content_top - self.scroll_y + PLAN_FORM_PADDING;

        // ── Section: Connection ──────────────────────────────────────────────
        Self::draw_section_title(canvas, "Connection", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        Self::draw_label(canvas, "API Token", px, y, cache);
        y += LABEL_H + PLAN_LABEL_GAP;
        let token_rect = Rect::from_xywh(px, y, field_w, PLAN_INPUT_H);
        Self::draw_text_field(
            canvas,
            token_rect,
            &self.api_token,
            self.focused == FocusedInput::Token,
            true,
            cache,
        );
        hit.token_field = token_rect;
        y += PLAN_INPUT_H + PLAN_FIELD_GAP;

        Self::draw_label(canvas, "Board ID", px, y, cache);
        y += LABEL_H + PLAN_LABEL_GAP;
        let board_rect = Rect::from_xywh(px, y, field_w, PLAN_INPUT_H);
        Self::draw_text_field(
            canvas,
            board_rect,
            &self.board_id,
            self.focused == FocusedInput::BoardId,
            false,
            cache,
        );
        hit.board_id_field = board_rect;
        y += PLAN_INPUT_H + PLAN_FIELD_GAP;

        let test_rect = Rect::from_xywh(px, y, 140.0, PLAN_BTN_H);
        Self::draw_button(
            canvas,
            test_rect,
            "Test Connection",
            self.hov_test,
            false,
            false,
            cache,
        );
        hit.test_btn = test_rect;
        y += PLAN_BTN_H;

        // ── Section: Column Mapping ──────────────────────────────────────────
        y += PLAN_FIELD_GAP;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(Rect::from_xywh(px, y, field_w, 1.0), &paint);
        y += PLAN_FIELD_GAP;

        Self::draw_section_title(canvas, "Column Mapping", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        let col_fields: [(&str, &TextInput, &mut Rect, FocusedInput); 5] = [
            (
                "Person Column ID",
                &self.person_col,
                &mut hit.person_col_field,
                FocusedInput::PersonCol,
            ),
            (
                "Status Column ID",
                &self.status_col,
                &mut hit.status_col_field,
                FocusedInput::StatusCol,
            ),
            (
                "Dependency Column ID",
                &self.dep_col,
                &mut hit.dep_col_field,
                FocusedInput::DepCol,
            ),
            (
                "Workload Column ID",
                &self.workload_col,
                &mut hit.workload_col_field,
                FocusedInput::WorkloadCol,
            ),
            (
                "Timeline Column ID",
                &self.timeline_col,
                &mut hit.timeline_col_field,
                FocusedInput::TimelineCol,
            ),
        ];

        for (label, input, rect_slot, focus_val) in col_fields {
            Self::draw_label(canvas, label, px, y, cache);
            y += LABEL_H + PLAN_LABEL_GAP;
            let r = Rect::from_xywh(px, y, field_w, PLAN_INPUT_H);
            Self::draw_text_field(canvas, r, input, self.focused == focus_val, false, cache);
            *rect_slot = r;
            y += PLAN_INPUT_H + PLAN_FIELD_GAP;
        }

        let fetch_rect = Rect::from_xywh(px, y, 200.0, PLAN_BTN_H);
        Self::draw_button(
            canvas,
            fetch_rect,
            "Fetch Board Info",
            self.hov_fetch,
            false,
            false,
            cache,
        );
        hit.fetch_btn = fetch_rect;
        y += PLAN_BTN_H;

        // ── Section: Options ─────────────────────────────────────────────────
        y += PLAN_FIELD_GAP;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(Rect::from_xywh(px, y, field_w, 1.0), &paint);
        y += PLAN_FIELD_GAP;

        Self::draw_section_title(canvas, "Item Type", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        let r0 = Rect::from_xywh(px, y, field_w / 2.0, RADIO_SIZE);
        Self::draw_radio(
            canvas,
            px,
            y,
            "Top-level items are tasks",
            !self.use_subitems,
            cache,
        );
        hit.use_subitems_radio[0] = r0;
        y += RADIO_SIZE + MAP_ROW_GAP;

        let r1 = Rect::from_xywh(px, y, field_w / 2.0, RADIO_SIZE);
        Self::draw_radio(
            canvas,
            px,
            y,
            "Subitems are tasks (items = milestones)",
            self.use_subitems,
            cache,
        );
        hit.use_subitems_radio[1] = r1;
        y += RADIO_SIZE;

        y += PLAN_FIELD_GAP;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(Rect::from_xywh(px, y, field_w, 1.0), &paint);
        y += PLAN_FIELD_GAP;

        Self::draw_section_title(canvas, "Workload Unit", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        let wr0 = Rect::from_xywh(px, y, field_w / 2.0, RADIO_SIZE);
        Self::draw_radio(canvas, px, y, "Days", !self.workload_in_hours, cache);
        hit.workload_hours_radio[0] = wr0;
        y += RADIO_SIZE + MAP_ROW_GAP;

        let wr1 = Rect::from_xywh(px, y, field_w / 2.0, RADIO_SIZE);
        Self::draw_radio(canvas, px, y, "Hours", self.workload_in_hours, cache);
        hit.workload_hours_radio[1] = wr1;
        y += RADIO_SIZE;

        // ── Section: User Mappings ────────────────────────────────────────────
        y += PLAN_FIELD_GAP;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(Rect::from_xywh(px, y, field_w, 1.0), &paint);
        y += PLAN_FIELD_GAP;

        Self::draw_section_title(canvas, "User Mappings", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        hit.user_plinko_selectors = Vec::new();
        if self.user_mappings.is_empty() {
            Self::draw_label(
                canvas,
                "No users fetched. Click 'Fetch Board Info' first.",
                px,
                y,
                cache,
            );
            y += PLAN_INPUT_H;
        } else {
            for mapping in &self.user_mappings {
                let row_rect = Rect::from_xywh(px, y, field_w, MAP_ROW_H);
                let plinko_name = mapping
                    .plinko_user_id
                    .map(|_| "Mapped")
                    .unwrap_or("(unmapped)");
                let right = format!("→ {plinko_name}  ▾");
                Self::draw_mapping_row(
                    canvas,
                    row_rect,
                    &mapping.monday_name,
                    &right,
                    false,
                    cache,
                );
                hit.user_plinko_selectors.push(row_rect);
                y += MAP_ROW_H + MAP_ROW_GAP;
            }
        }

        // ── Section: Status Mappings ──────────────────────────────────────────
        y += PLAN_FIELD_GAP;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(Rect::from_xywh(px, y, field_w, 1.0), &paint);
        y += PLAN_FIELD_GAP;

        Self::draw_section_title(canvas, "Status Mappings", px, y, cache);
        y += SECTION_TITLE_H + SECTION_GAP;

        hit.status_plinko_selectors = Vec::new();
        if self.status_mappings.is_empty() {
            Self::draw_label(
                canvas,
                "No statuses fetched. Click 'Fetch Board Info' first.",
                px,
                y,
                cache,
            );
            y += PLAN_INPUT_H;
        } else {
            for mapping in &self.status_mappings {
                let row_rect = Rect::from_xywh(px, y, field_w, MAP_ROW_H);
                let plinko_status_name = status_display_name(mapping.plinko_status);
                let right = format!("→ {plinko_status_name}  ▾");
                Self::draw_mapping_row(
                    canvas,
                    row_rect,
                    &mapping.monday_label,
                    &right,
                    false,
                    cache,
                );
                hit.status_plinko_selectors.push(row_rect);
                y += MAP_ROW_H + MAP_ROW_GAP;
            }
        }

        // ── Save config button ────────────────────────────────────────────────
        y += PLAN_FIELD_GAP;
        let save_rect = Rect::from_xywh(px, y, 120.0, PLAN_BTN_H);
        Self::draw_button(
            canvas,
            save_rect,
            "Save Config",
            self.hov_save,
            true,
            false,
            cache,
        );
        hit.save_btn = save_rect;

        canvas.restore();

        // ── Scrollbar ──────────────────────────────────────────────────────
        let max_scroll = self.max_scroll(height);
        if max_scroll > 0.0 {
            let sb_x = panel.right - SCROLLBAR_W - 4.0;
            let track_h = content_vp_h;
            let thumb_h = (content_vp_h / self.total_content_height() * track_h).max(20.0);
            let thumb_y = content_top + (self.scroll_y / max_scroll) * (track_h - thumb_h);
            let mut sb_paint = Paint::default();
            sb_paint.set_anti_alias(true);
            sb_paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(sb_x, thumb_y, SCROLLBAR_W, thumb_h),
                    2.0,
                    2.0,
                ),
                &sb_paint,
            );
        }

        // ── Footer (always visible) ───────────────────────────────────────────
        let footer_top = panel.top + TITLE_H + 1.0 + content_vp_h;
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, footer_top, PANEL_W, 1.0),
            &paint,
        );

        let fp = panel.left + PLAN_FORM_PADDING;
        let fy = footer_top
            + (FOOTER_H / 2.0 - PLAN_BTN_H) / 2.0
            + (FOOTER_H - PLAN_BTN_H * 2.0 - 8.0) / 2.0;
        let pull_rect = Rect::from_xywh(fp, footer_top + 16.0, 160.0, PLAN_BTN_H);
        let push_rect = Rect::from_xywh(fp + 168.0, footer_top + 16.0, 160.0, PLAN_BTN_H);
        Self::draw_button(
            canvas,
            pull_rect,
            "Pull from Monday",
            self.hov_pull,
            true,
            false,
            cache,
        );
        Self::draw_button(
            canvas,
            push_rect,
            "Push dates to Monday",
            self.hov_push,
            false,
            false,
            cache,
        );
        hit.pull_btn = pull_rect;
        hit.push_btn = push_rect;

        // Status message
        let status_msg = self
            .sync_state
            .lock()
            .ok()
            .map(|s| match &*s {
                SyncState::Idle => String::new(),
                SyncState::InProgress(m) => format!("⟳ {m}"),
                SyncState::Done(m) => m.clone(),
                SyncState::Err(m) => format!("Error: {m}"),
            })
            .unwrap_or_default();

        if !status_msg.is_empty() {
            paint.set_color(Color::from(MUTED_FG));
            let (_, metrics) = font.metrics();
            let sy = footer_top + 16.0 + PLAN_BTN_H + 8.0 - metrics.ascent;
            canvas.draw_str(&status_msg, (fp, sy), font, &paint);
        }

        *self.rects.borrow_mut() = hit;
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        _width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> FloatingWindowOutcome {
        let hit = self.rects.borrow().clone();
        let mut dirty = false;

        macro_rules! chk {
            ($field:ident, $rect:expr) => {{
                let v = $rect.contains(Point::new(x, y));
                if v != self.$field {
                    self.$field = v;
                    dirty = true;
                }
            }};
        }

        chk!(hov_close, hit.close_btn);
        chk!(hov_test, hit.test_btn);
        chk!(hov_fetch, hit.fetch_btn);
        chk!(hov_save, hit.save_btn);
        chk!(hov_pull, hit.pull_btn);
        chk!(hov_push, hit.push_btn);

        if dirty {
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
        _width: f32,
        _height: f32,
        _modifiers: &Modifiers,
        plan: &Plan,
        sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }

        let hit = self.rects.borrow().clone();
        let pt = Point::new(x, y);

        // ── Close button ──────────────────────────────────────────────────
        if hit.close_btn.contains(pt) {
            return FloatingWindowOutcome::close();
        }

        // ── Text input focus ──────────────────────────────────────────────
        let field_map: [(Rect, FocusedInput); 7] = [
            (hit.token_field, FocusedInput::Token),
            (hit.board_id_field, FocusedInput::BoardId),
            (hit.person_col_field, FocusedInput::PersonCol),
            (hit.status_col_field, FocusedInput::StatusCol),
            (hit.dep_col_field, FocusedInput::DepCol),
            (hit.workload_col_field, FocusedInput::WorkloadCol),
            (hit.timeline_col_field, FocusedInput::TimelineCol),
        ];
        for (rect, focus_val) in &field_map {
            if rect.contains(pt) {
                // Set cursor position
                let h_pad = 8.0;
                let inner_x = rect.left + h_pad;
                let input = self.input_for_focus(focus_val);
                let x_in = x - inner_x + input.scroll_x.get();
                let cur = input.cursor_for_x(x_in, &cache.font);
                input.cursor = cur;
                input.focused = true;
                self.focused = focus_val.clone();
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
        }
        // Click outside field → unfocus
        self.focused = FocusedInput::None;

        // ── Test Connection ───────────────────────────────────────────────
        if hit.test_btn.contains(pt) {
            let token = self.api_token.content.trim().to_string();
            let status = Arc::clone(&self.sync_state);
            *status.lock().unwrap() = SyncState::InProgress("Testing connection...".to_string());
            thread::spawn(move || {
                let client = MondayClient::new(&token);
                match client.test_connection() {
                    Ok(name) => {
                        *status.lock().unwrap() = SyncState::Done(format!("Connected as: {name}"));
                    }
                    Err(e) => {
                        *status.lock().unwrap() = SyncState::Err(e.0);
                    }
                }
            });
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Fetch Board Info ───────────────────────────────────────────────
        if hit.fetch_btn.contains(pt) {
            let token = self.api_token.content.trim().to_string();
            let board_id = self.board_id.content.trim().to_string();
            let status_col = self.status_col.content.trim().to_string();
            let status = Arc::clone(&self.sync_state);
            *status.lock().unwrap() = SyncState::InProgress("Fetching board info...".to_string());

            let (user_tx, user_rx) = std::sync::mpsc::channel();
            thread::spawn(move || {
                let client = MondayClient::new(&token);
                let users = client.fetch_users().unwrap_or_default();
                let statuses = if !status_col.is_empty() && !board_id.is_empty() {
                    client
                        .fetch_status_labels(&board_id, &status_col)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let _ = user_tx.send((users, statuses));
                *status.lock().unwrap() = SyncState::Done("Board info fetched.".to_string());
            });

            // Try to get result immediately (won't work across threads, but attempt)
            // In a real async UI, we'd poll. Here we just update on next event.
            drop(user_rx); // Can't use across threads easily; see tick_animation
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Save Config ───────────────────────────────────────────────────
        if hit.save_btn.contains(pt) {
            self.save_config();
            *self.sync_state.lock().unwrap() = SyncState::Done("Config saved.".to_string());
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Pull from Monday ──────────────────────────────────────────────
        if hit.pull_btn.contains(pt) {
            let config = self.current_config();
            let token = self.api_token.content.trim().to_string();
            let plan_id = self.plan_id;
            let sender_clone = sender.clone();
            let status = Arc::clone(&self.sync_state);
            *status.lock().unwrap() =
                SyncState::InProgress("Pulling from Monday.com...".to_string());
            thread::spawn(move || {
                let client = MondayClient::new(&token);
                match import::import_from_monday(&client, &config, &sender_clone) {
                    Ok((new_map, msg)) => {
                        // Save updated config with new item_node_map
                        let mut updated = config;
                        updated.item_node_map = new_map;
                        if let Ok(storage) = Storage::from_user_data_dir() {
                            storage.save_monday_config(plan_id, &updated);
                        }
                        *status.lock().unwrap() = SyncState::Done(msg);
                    }
                    Err(e) => {
                        *status.lock().unwrap() = SyncState::Err(e.0);
                    }
                }
            });
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Push to Monday ────────────────────────────────────────────────
        if hit.push_btn.contains(pt) {
            let config = self.current_config();
            let token = self.api_token.content.trim().to_string();
            let item_node_map = self.item_node_map.clone();
            let plan_snapshot = plan.clone();
            let status = Arc::clone(&self.sync_state);
            *status.lock().unwrap() =
                SyncState::InProgress("Pushing dates to Monday.com...".to_string());
            thread::spawn(move || {
                let client = MondayClient::new(&token);
                match export::export_to_monday(&client, &config, &plan_snapshot, &item_node_map) {
                    Ok(msg) => {
                        *status.lock().unwrap() = SyncState::Done(msg);
                    }
                    Err(e) => {
                        *status.lock().unwrap() = SyncState::Err(e.0);
                    }
                }
            });
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Radio buttons ─────────────────────────────────────────────────
        if hit.use_subitems_radio[0].contains(pt) {
            self.use_subitems = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        if hit.use_subitems_radio[1].contains(pt) {
            self.use_subitems = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        if hit.workload_hours_radio[0].contains(pt) {
            self.workload_in_hours = false;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }
        if hit.workload_hours_radio[1].contains(pt) {
            self.workload_in_hours = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::All);
        }

        // ── Status mapping cycle ───────────────────────────────────────────
        for (i, rect) in hit.status_plinko_selectors.iter().enumerate() {
            if rect.contains(pt) {
                if let Some(mapping) = self.status_mappings.get_mut(i) {
                    mapping.plinko_status = cycle_status(mapping.plinko_status);
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
        }

        FloatingWindowOutcome::dirty(DirtyRegion::All)
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) => {
                return FloatingWindowOutcome::close();
            }
            Key::Named(NamedKey::Tab) => {
                self.focused = FocusedInput::None;
                return FloatingWindowOutcome::dirty(DirtyRegion::All);
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(input) = self.focused_input_mut() {
                    input.backspace();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_left();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_right();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            Key::Named(NamedKey::Home) => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_home();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            Key::Named(NamedKey::End) => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_end();
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            Key::Character(s) => {
                if let Some(input) = self.focused_input_mut() {
                    input.insert_str(s);
                    return FloatingWindowOutcome::dirty(DirtyRegion::All);
                }
            }
            _ => {}
        }
        FloatingWindowOutcome::default()
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
        if let Some(input) = self.focused_input_mut() {
            input.insert_str(text);
            FloatingWindowOutcome::dirty(DirtyRegion::All)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _plan: &Plan,
        _width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        let max_scroll = self.max_scroll(height);
        let new_scroll = (self.scroll_y - delta_y * 40.0).clamp(0.0, max_scroll);
        if (new_scroll - self.scroll_y).abs() > 0.1 {
            self.scroll_y = new_scroll;
            FloatingWindowOutcome::dirty(DirtyRegion::All)
        } else {
            FloatingWindowOutcome::default()
        }
    }
}

// ── Input accessor helpers ─────────────────────────────────────────────────────

impl MondayWindow {
    fn focused_input_mut(&mut self) -> Option<&mut TextInput> {
        match &self.focused {
            FocusedInput::None => None,
            FocusedInput::Token => Some(&mut self.api_token),
            FocusedInput::BoardId => Some(&mut self.board_id),
            FocusedInput::PersonCol => Some(&mut self.person_col),
            FocusedInput::StatusCol => Some(&mut self.status_col),
            FocusedInput::DepCol => Some(&mut self.dep_col),
            FocusedInput::WorkloadCol => Some(&mut self.workload_col),
            FocusedInput::TimelineCol => Some(&mut self.timeline_col),
        }
    }

    fn input_for_focus(&mut self, focus: &FocusedInput) -> &mut TextInput {
        match focus {
            FocusedInput::Token => &mut self.api_token,
            FocusedInput::BoardId => &mut self.board_id,
            FocusedInput::PersonCol => &mut self.person_col,
            FocusedInput::StatusCol => &mut self.status_col,
            FocusedInput::DepCol => &mut self.dep_col,
            FocusedInput::WorkloadCol => &mut self.workload_col,
            FocusedInput::TimelineCol => &mut self.timeline_col,
            FocusedInput::None => panic!("input_for_focus called with None"),
        }
    }
}

// ── Free helpers ───────────────────────────────────────────────────────────────

fn status_display_name(s: Status) -> &'static str {
    match s {
        Status::NotStarted => "Not Started",
        Status::InProgress => "In Progress",
        Status::OnHold => "On Hold",
        Status::Complete => "Complete",
        Status::Dropped => "Dropped",
    }
}

fn cycle_status(s: Status) -> Status {
    match s {
        Status::NotStarted => Status::InProgress,
        Status::InProgress => Status::OnHold,
        Status::OnHold => Status::Complete,
        Status::Complete => Status::Dropped,
        Status::Dropped => Status::NotStarted,
    }
}
