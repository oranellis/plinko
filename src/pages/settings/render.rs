//! Rendering functions for the settings page.

use skia_safe::{Canvas, Color, Paint, PaintStyle, RRect, Rect, TextBlob};

use crate::data::Plan;
use crate::ui::cache::RenderCache;
use crate::ui::layout::{
    ADD_BTN_BG, ADD_BTN_FG, ADD_BTN_HOVER_BG, BACK_BTN_SIZE, BACK_BTN_X, BACK_BTN_Y,
    BTN_PRIMARY_BG, BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, MUTED_FG, PANEL_BG,
    SCROLLBAR_THUMB_COLOR,
};

use super::state::{PlanEntry, SettingsState};

// ── Layout ────────────────────────────────────────────────────────────────────

pub const CONTENT_TOP: f32 = BACK_BTN_Y + BACK_BTN_SIZE + 16.0;
const SIDE_PAD: f32 = BACK_BTN_X;
const SECTION_TITLE_H: f32 = 20.0;
const SECTION_GAP: f32 = 12.0;
const DIVIDER_GAP: f32 = 24.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_CORNER: f32 = 5.0;
pub const ROW_H: f32 = 36.0;
const ROW_CORNER: f32 = 4.0;
const SCROLLBAR_W: f32 = 6.0;
const SCROLLBAR_PAD: f32 = 4.0;

/// Total scrollable content height.
pub fn total_content_height(plan: &Plan, plan_list: &[PlanEntry]) -> f32 {
    // Plan Management section
    let plan_mgmt_h = SECTION_TITLE_H
        + SECTION_GAP
        + BUTTON_H
        + SECTION_GAP
        + SECTION_GAP  // "Saved Plans" label row
        + plan_list.len().max(1) as f32 * ROW_H;

    // Identity section
    let users_count = plan.users.len();
    let identity_h = SECTION_TITLE_H + SECTION_GAP + (users_count + 1) as f32 * ROW_H;

    plan_mgmt_h + DIVIDER_GAP * 2.0 + identity_h + DIVIDER_GAP
}

fn sorted_users(plan: &Plan) -> Vec<(crate::data::ids::UserId, String)> {
    let mut users: Vec<_> = plan
        .users
        .iter()
        .map(|(id, u)| (*id, u.name.clone()))
        .collect();
    users.sort_by(|a, b| a.1.cmp(&b.1));
    users
}

// ── Hit test helpers (pub for mod.rs) ─────────────────────────────────────────

fn btns_y() -> f32 {
    CONTENT_TOP + SECTION_TITLE_H + SECTION_GAP
}

pub fn save_btn_rect(width: f32) -> Rect {
    let btn_w = (width - 2.0 * SIDE_PAD - 8.0) / 2.0;
    Rect::from_xywh(SIDE_PAD, btns_y(), btn_w, BUTTON_H)
}

pub fn new_btn_rect(width: f32) -> Rect {
    let btn_w = (width - 2.0 * SIDE_PAD - 8.0) / 2.0;
    Rect::from_xywh(SIDE_PAD + btn_w + 8.0, btns_y(), btn_w, BUTTON_H)
}

/// Top of the plan list rows (before scroll).
fn plan_rows_top_raw() -> f32 {
    CONTENT_TOP + SECTION_TITLE_H + SECTION_GAP + BUTTON_H + SECTION_GAP + SECTION_GAP // "Saved Plans" label
}

/// Y position of plan row `idx` accounting for scroll.
pub fn plan_row_y(idx: usize, scroll_y: f32) -> f32 {
    plan_rows_top_raw() + idx as f32 * ROW_H - scroll_y
}

pub fn plan_row_rect(idx: usize, scroll_y: f32, width: f32) -> Rect {
    let y = plan_row_y(idx, scroll_y);
    Rect::from_xywh(SIDE_PAD, y, width - 2.0 * SIDE_PAD, ROW_H)
}

pub fn load_btn_rect(idx: usize, scroll_y: f32, width: f32) -> Rect {
    let row = plan_row_rect(idx, scroll_y, width);
    let btn_w = 70.0_f32;
    Rect::from_xywh(
        row.right() - btn_w - 4.0,
        row.top() + (ROW_H - BUTTON_H) / 2.0,
        btn_w,
        BUTTON_H,
    )
}

fn identity_top_raw(plan_list_len: usize) -> f32 {
    plan_rows_top_raw() + plan_list_len.max(1) as f32 * ROW_H + DIVIDER_GAP
}

pub fn identity_section_y(plan_list_len: usize, scroll_y: f32) -> f32 {
    identity_top_raw(plan_list_len) - scroll_y
}

pub fn user_row_rect(idx: usize, plan_list_len: usize, scroll_y: f32, width: f32) -> Rect {
    let top = identity_section_y(plan_list_len, scroll_y) + SECTION_TITLE_H + SECTION_GAP;
    Rect::from_xywh(
        SIDE_PAD,
        top + idx as f32 * ROW_H,
        width - 2.0 * SIDE_PAD,
        ROW_H,
    )
}

// ── Main draw ─────────────────────────────────────────────────────────────────

pub fn draw_settings(
    canvas: &Canvas,
    width: f32,
    height: f32,
    state: &SettingsState,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    paint.set_color(Color::from(PANEL_BG));
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, width, height), &paint);

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(0.0, CONTENT_TOP, width, height - CONTENT_TOP),
        None,
        None,
    );

    draw_plan_section(canvas, width, state, &mut paint, cache);
    draw_identity_section(canvas, width, state, plan, &mut paint, cache);

    canvas.restore();

    draw_scrollbar(canvas, width, height, state, plan, &mut paint);
}

// ── Plan Management ───────────────────────────────────────────────────────────

fn draw_plan_section(
    canvas: &Canvas,
    width: f32,
    state: &SettingsState,
    paint: &mut Paint,
    cache: &RenderCache,
) {
    // Section title
    paint.set_color(Color::from(0xff_555555_u32));
    if let Some(blob) = TextBlob::new("Plan Management", &cache.font) {
        canvas.draw_text_blob(
            &blob,
            (SIDE_PAD, CONTENT_TOP + blob.bounds().height()),
            paint,
        );
    }

    let by = btns_y();
    let btn_w = (width - 2.0 * SIDE_PAD - 8.0) / 2.0;

    draw_button(
        canvas,
        Rect::from_xywh(SIDE_PAD, by, btn_w, BUTTON_H),
        "Save Plan",
        state.hovered_save,
        false,
        paint,
        cache,
    );
    draw_button(
        canvas,
        Rect::from_xywh(SIDE_PAD + btn_w + 8.0, by, btn_w, BUTTON_H),
        "New Plan",
        state.hovered_new,
        false,
        paint,
        cache,
    );

    // "Saved Plans" sub-label
    let sub_y = by + BUTTON_H + SECTION_GAP;
    paint.set_color(Color::from(MUTED_FG));
    if let Some(blob) = TextBlob::new("Saved Plans", &cache.small_font) {
        canvas.draw_text_blob(&blob, (SIDE_PAD, sub_y + blob.bounds().height()), paint);
    }

    // Plan rows
    for (idx, entry) in state.plan_list.iter().enumerate() {
        draw_plan_row(canvas, idx, entry, state, width, paint, cache);
    }

    if state.plan_list.is_empty() {
        let empty_y = plan_rows_top_raw() - state.scroll_y + 10.0;
        paint.set_color(Color::from(MUTED_FG));
        if let Some(blob) = TextBlob::new("No saved plans", &cache.small_font) {
            canvas.draw_text_blob(
                &blob,
                (SIDE_PAD + 8.0, empty_y + blob.bounds().height()),
                paint,
            );
        }
    }
}

fn draw_plan_row(
    canvas: &Canvas,
    idx: usize,
    entry: &PlanEntry,
    state: &SettingsState,
    width: f32,
    paint: &mut Paint,
    cache: &RenderCache,
) {
    let row_y = plan_row_y(idx, state.scroll_y);
    let row_rect = Rect::from_xywh(SIDE_PAD, row_y, width - 2.0 * SIDE_PAD, ROW_H);

    let hovered = state.hovered_plan_row == Some(idx);
    if hovered || entry.is_current {
        paint.set_color(Color::from(if entry.is_current {
            0xff_e8f0fe_u32
        } else {
            ADD_BTN_BG
        }));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(RRect::new_rect_xy(row_rect, ROW_CORNER, ROW_CORNER), paint);
    }

    // Plan name
    paint.set_color(Color::from(if entry.is_current {
        BTN_PRIMARY_BG
    } else {
        0xff_333333_u32
    }));
    paint.set_style(PaintStyle::Fill);
    if let Some(blob) = TextBlob::new(&entry.name, &cache.font) {
        let ty = row_y + (ROW_H + blob.bounds().height()) / 2.0;
        canvas.draw_text_blob(&blob, (SIDE_PAD + 8.0, ty), paint);
    }

    // Timestamp
    paint.set_color(Color::from(MUTED_FG));
    if let Some(blob) = TextBlob::new(&entry.last_saved, &cache.small_font) {
        // Place it after the name but left of the button
        let tx = width / 2.0;
        let ty = row_y + (ROW_H + blob.bounds().height()) / 2.0;
        canvas.draw_text_blob(&blob, (tx, ty), paint);
    }

    // Load / Current
    if !entry.is_current {
        draw_button(
            canvas,
            load_btn_rect(idx, state.scroll_y, width),
            "Load",
            state.hovered_load_btn == Some(idx),
            true,
            paint,
            cache,
        );
    } else if let Some(blob) = TextBlob::new("current", &cache.small_font) {
        paint.set_color(Color::from(BTN_PRIMARY_BG));
        let bx = row_rect.right() - blob.bounds().width() - 12.0;
        let by = row_y + (ROW_H + blob.bounds().height()) / 2.0;
        canvas.draw_text_blob(&blob, (bx, by), paint);
    }
}

// ── Identity section ──────────────────────────────────────────────────────────

fn draw_identity_section(
    canvas: &Canvas,
    width: f32,
    state: &SettingsState,
    plan: &Plan,
    paint: &mut Paint,
    cache: &RenderCache,
) {
    let section_y = identity_section_y(state.plan_list.len(), state.scroll_y);

    // Divider
    paint.set_color(Color::from(0xff_e0e0e0_u32));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_line(
        (SIDE_PAD, section_y - DIVIDER_GAP / 2.0),
        (width - SIDE_PAD, section_y - DIVIDER_GAP / 2.0),
        paint,
    );
    paint.set_style(PaintStyle::Fill);

    // Title
    paint.set_color(Color::from(0xff_555555_u32));
    if let Some(blob) = TextBlob::new("Identity", &cache.font) {
        canvas.draw_text_blob(&blob, (SIDE_PAD, section_y + blob.bounds().height()), paint);
    }

    paint.set_color(Color::from(MUTED_FG));
    if let Some(blob) = TextBlob::new(
        "Who am I? (used to highlight your tasks)",
        &cache.small_font,
    ) {
        let ty = section_y + SECTION_TITLE_H + blob.bounds().height();
        canvas.draw_text_blob(&blob, (SIDE_PAD, ty), paint);
    }

    let rows_top = section_y + SECTION_TITLE_H + SECTION_GAP;
    let users = sorted_users(plan);

    for (idx, (uid, name)) in users.iter().enumerate() {
        draw_user_row(
            canvas,
            idx,
            name,
            Some(*uid),
            state,
            rows_top,
            width,
            paint,
            cache,
        );
    }

    // "None" option
    draw_user_row(
        canvas,
        users.len(),
        "(no user)",
        None,
        state,
        rows_top,
        width,
        paint,
        cache,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_user_row(
    canvas: &Canvas,
    idx: usize,
    name: &str,
    uid: Option<crate::data::ids::UserId>,
    state: &SettingsState,
    rows_top: f32,
    width: f32,
    paint: &mut Paint,
    cache: &RenderCache,
) {
    let row_y = rows_top + idx as f32 * ROW_H;
    let row_rect = Rect::from_xywh(SIDE_PAD, row_y, width - 2.0 * SIDE_PAD, ROW_H);

    let is_selected = uid
        .map(|id| state.current_user == Some(id))
        .unwrap_or(state.current_user.is_none());
    let is_hovered = state.hovered_user_idx == Some(idx);

    if is_selected || is_hovered {
        paint.set_color(Color::from(if is_selected {
            0xff_e8f0fe_u32
        } else {
            ADD_BTN_BG
        }));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(RRect::new_rect_xy(row_rect, ROW_CORNER, ROW_CORNER), paint);
    }

    let dot_cx = SIDE_PAD + 14.0;
    let dot_cy = row_y + ROW_H / 2.0;
    paint.set_color(Color::from(if is_selected {
        BTN_PRIMARY_BG
    } else {
        0xff_bbbbbb_u32
    }));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    canvas.draw_circle((dot_cx, dot_cy), 7.0, paint);
    if is_selected {
        paint.set_style(PaintStyle::Fill);
        canvas.draw_circle((dot_cx, dot_cy), 4.0, paint);
    }

    paint.set_style(PaintStyle::Fill);
    paint.set_color(Color::from(if is_selected {
        BTN_PRIMARY_BG
    } else {
        0xff_333333_u32
    }));
    if let Some(blob) = TextBlob::new(name, &cache.font) {
        let ty = row_y + (ROW_H + blob.bounds().height()) / 2.0;
        canvas.draw_text_blob(&blob, (SIDE_PAD + 28.0, ty), paint);
    }
}

// ── Shared button ─────────────────────────────────────────────────────────────

fn draw_button(
    canvas: &Canvas,
    rect: Rect,
    label: &str,
    hovered: bool,
    secondary: bool,
    paint: &mut Paint,
    cache: &RenderCache,
) {
    let (bg, fg) = if secondary {
        (
            if hovered {
                ADD_BTN_HOVER_BG
            } else {
                ADD_BTN_BG
            },
            ADD_BTN_FG,
        )
    } else {
        (
            if hovered {
                BTN_PRIMARY_HOVER_BG
            } else {
                BTN_PRIMARY_BG
            },
            BTN_PRIMARY_FG,
        )
    };
    paint.set_color(Color::from(bg));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(rect, BUTTON_CORNER, BUTTON_CORNER),
        paint,
    );
    paint.set_color(Color::from(fg));
    if let Some(blob) = TextBlob::new(label, &cache.small_font) {
        let tx = rect.left() + (rect.width() - blob.bounds().width()) / 2.0 - blob.bounds().left();
        let ty = rect.top() + (rect.height() + blob.bounds().height()) / 2.0;
        canvas.draw_text_blob(&blob, (tx, ty), paint);
    }
}

// ── Scrollbar ─────────────────────────────────────────────────────────────────

fn draw_scrollbar(
    canvas: &Canvas,
    width: f32,
    height: f32,
    state: &SettingsState,
    plan: &Plan,
    paint: &mut Paint,
) {
    let content_h = total_content_height(plan, &state.plan_list);
    let viewport_h = height - CONTENT_TOP;
    if content_h <= viewport_h {
        return;
    }
    let max_scroll = content_h - viewport_h;
    let thumb_h = ((viewport_h / content_h) * viewport_h).max(40.0);
    let thumb_y = CONTENT_TOP + (state.scroll_y / max_scroll) * (viewport_h - thumb_h);
    let x = width - SCROLLBAR_W - SCROLLBAR_PAD;

    paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(x, thumb_y, SCROLLBAR_W, thumb_h),
            SCROLLBAR_W / 2.0,
            SCROLLBAR_W / 2.0,
        ),
        paint,
    );
}
