//! Floating form for creating or editing a team member.
//!
//! A single [`UserFormWindow`] handles both flows; the title and submit
//! behaviour change based on whether a `UserId` was supplied at construction.

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::keyboard::{Key, NamedKey};

use rfd;

use crate::data::{Plan, Tag, TagId, User, ids::UserId};
use crate::engine::{PlanRequest, PlanRequestSender, UserPatch};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, DIVIDER_COLOR, INPUT_BG, INPUT_BORDER,
    INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG,
    LIST_BG, LIST_ITEM_HOVER_BG, MUTED_FG, OVERLAY_SOFT, OVERLAY_XLIGHT, PANEL_BG, PANEL_TEXT,
    PLACEHOLDER_FG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING, PLAN_INPUT_H,
    PLAN_LABEL_GAP, TOOLBAR_STROKE_WIDTH,
};
use crate::ui::text_input::TextInput;

const PANEL_W: f32 = 420.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const LABEL_H: f32 = 14.0;
const FIELD_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H;
const PANEL_H: f32 = TITLE_H
    + 1.0
    + PLAN_FORM_PADDING
    + FIELD_BLOCK_H   // name
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // tags trigger
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // avatar path
    + LABEL_H         // error row
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;
const SAVE_BTN_W: f32 = 80.0;

const DROPDOWN_FILTER_H: f32 = PLAN_INPUT_H;
const DROPDOWN_ROW_H: f32 = 28.0;
const MAX_DROPDOWN_ROWS: usize = 4;
const DROPDOWN_H: f32 = DROPDOWN_FILTER_H + MAX_DROPDOWN_ROWS as f32 * DROPDOWN_ROW_H;

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    TagFilter,
    AvatarPath,
}

/// Whether this form is creating a new user or editing an existing one.
enum Mode {
    New,
    Edit(UserId),
}

pub struct UserFormWindow {
    mode: Mode,
    name: TextInput,
    tag_filter: TextInput,
    selected_tags: Vec<TagId>,
    dropdown_open: bool,
    dropdown_scroll: usize,
    dropdown_hovered: Option<usize>,
    avatar_path: TextInput,
    focused: Field,
    hovered_back: bool,
    hovered_save: bool,
    hovered_browse: bool,
    avatar_error: bool,
    name_error: bool,
}

impl UserFormWindow {
    /// Open the form to create a new team member.
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            tag_filter: TextInput::new(""),
            selected_tags: Vec::new(),
            dropdown_open: false,
            dropdown_scroll: 0,
            dropdown_hovered: None,
            avatar_path: TextInput::new(""),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
            hovered_browse: false,
            avatar_error: false,
            name_error: false,
        }
    }

    /// Open the form pre-filled with an existing user's data.
    pub fn from_user(user: &User) -> Self {
        let mut name = TextInput::new(&user.name);
        name.focused = true;
        let selected_tags: Vec<TagId> = user.tags.iter().copied().collect();
        Self {
            mode: Mode::Edit(user.id),
            name,
            tag_filter: TextInput::new(""),
            selected_tags,
            dropdown_open: false,
            dropdown_scroll: 0,
            dropdown_hovered: None,
            avatar_path: TextInput::new(""),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
            hovered_browse: false,
            avatar_error: false,
            name_error: false,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::New => "Add Team Member",
            Mode::Edit(_) => "Edit Team Member",
        }
    }

    fn avatar_label(&self) -> &'static str {
        match self.mode {
            Mode::New => "Avatar image path (optional)",
            Mode::Edit(_) => "Avatar image path (blank = keep existing)",
        }
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph)
    }

    fn back_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.left + BTN_INSET,
            panel.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn save_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.right - PLAN_FORM_PADDING - SAVE_BTN_W,
            panel.bottom - PLAN_FORM_PADDING - PLAN_BTN_H,
            SAVE_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn browse_btn_rect(width: f32, height: f32) -> Rect {
        let full = Self::input_rect(Field::AvatarPath, width, height);
        const BROWSE_W: f32 = 72.0;
        const GAP: f32 = 6.0;
        Rect::from_xywh(full.right - BROWSE_W, full.top, BROWSE_W, full.height())
    }

    fn avatar_input_rect(width: f32, height: f32) -> Rect {
        let full = Self::input_rect(Field::AvatarPath, width, height);
        const BROWSE_W: f32 = 72.0;
        const GAP: f32 = 6.0;
        Rect::from_xywh(
            full.left,
            full.top,
            full.width() - BROWSE_W - GAP,
            full.height(),
        )
    }

    fn form_top(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn input_rect(field: Field, width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        let x = panel.left + PLAN_FORM_PADDING;
        let w = panel.width() - 2.0 * PLAN_FORM_PADDING;
        let y0 = Self::form_top(width, height);
        let y = match field {
            Field::Name => y0 + LABEL_H + PLAN_LABEL_GAP,
            Field::TagFilter => y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP,
            Field::AvatarPath => {
                y0 + 2.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP) + LABEL_H + PLAN_LABEL_GAP
            }
        };
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn dropdown_rect(width: f32, height: f32) -> Rect {
        let trigger = Self::input_rect(Field::TagFilter, width, height);
        Rect::from_xywh(
            trigger.left,
            trigger.bottom + 4.0,
            trigger.width(),
            DROPDOWN_H,
        )
    }

    fn dropdown_list_top(width: f32, height: f32) -> f32 {
        Self::dropdown_rect(width, height).top + DROPDOWN_FILTER_H
    }

    fn set_focus(&mut self, field: Field) {
        self.name.focused = field == Field::Name;
        self.tag_filter.focused = field == Field::TagFilter;
        self.avatar_path.focused = field == Field::AvatarPath;
        self.focused = field;
    }

    fn cycle_focus_forward(&mut self) {
        self.close_dropdown();
        let next = match self.focused {
            Field::Name => Field::TagFilter,
            Field::TagFilter => Field::AvatarPath,
            Field::AvatarPath => Field::Name,
        };
        self.set_focus(next);
        if next == Field::TagFilter {
            self.open_dropdown();
        }
    }

    fn open_dropdown(&mut self) {
        self.dropdown_open = true;
        self.dropdown_scroll = 0;
        self.dropdown_hovered = None;
        self.set_focus(Field::TagFilter);
    }

    fn close_dropdown(&mut self) {
        self.dropdown_open = false;
        self.dropdown_hovered = None;
    }

    fn filtered_tags<'a>(&self, plan: &'a Plan) -> Vec<&'a Tag> {
        let filter = self.tag_filter.content.to_lowercase();
        plan.tags
            .iter()
            .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()))
            .collect()
    }

    fn toggle_tag(&mut self, tag_id: TagId) {
        if let Some(pos) = self.selected_tags.iter().position(|&id| id == tag_id) {
            self.selected_tags.remove(pos);
        } else {
            self.selected_tags.push(tag_id);
        }
    }

    fn try_submit(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self.name.content.trim().to_string();
        self.name_error = name.is_empty();

        let avatar_path = self.avatar_path.content.trim();
        let avatar_read: Result<Option<Vec<u8>>, ()> = if avatar_path.is_empty() {
            Ok(None)
        } else {
            std::fs::read(avatar_path).map(Some).map_err(|_| ())
        };
        self.avatar_error = avatar_read.is_err();

        if self.name_error || self.avatar_error {
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        let avatar_bytes = avatar_read.unwrap();

        match self.mode {
            Mode::New => {
                let mut user = User::new(name);
                for &tag_id in &self.selected_tags {
                    user.add_tag(tag_id);
                }
                user.avatar = avatar_bytes;
                sender.send(PlanRequest::CreateUser(user));
            }
            Mode::Edit(user_id) => {
                let tags: std::collections::HashSet<TagId> =
                    self.selected_tags.iter().copied().collect();
                let mut patch = UserPatch::new().name(name).tags(tags);
                if let Some(bytes) = avatar_bytes {
                    patch = patch.avatar(Some(bytes));
                }
                sender.send(PlanRequest::UpdateUser(user_id, patch));
            }
        }
        FloatingWindowOutcome::close()
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused {
            Field::Name => &mut self.name,
            Field::TagFilter => &mut self.tag_filter,
            Field::AvatarPath => &mut self.avatar_path,
        }
    }
}

// ── Drawing helpers ───────────────────────────────────────────────────────────

fn draw_chevron_btn(canvas: &Canvas, btn_rect: Rect, hovered: bool) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if hovered {
        paint.set_color(Color::from(BACK_BTN_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(btn_rect, BACK_BTN_CORNER, BACK_BTN_CORNER),
            &paint,
        );
    }
    let cx = btn_rect.left + BACK_BTN_SIZE / 2.0;
    let cy = btn_rect.top + BACK_BTN_SIZE / 2.0;
    let aw = BACK_BTN_SIZE * 0.3;
    let ah = BACK_BTN_SIZE * 0.3;
    let mut pb = PathBuilder::new();
    pb.move_to((cx + aw / 2.0, cy - ah / 2.0));
    pb.line_to((cx - aw / 2.0, cy));
    pb.line_to((cx + aw / 2.0, cy + ah / 2.0));
    paint.set_color(Color::from(BACK_BTN_ICON_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
    canvas.draw_path(&pb.detach(), &paint);
}

fn draw_text_input(
    canvas: &Canvas,
    rect: Rect,
    input: &TextInput,
    focused: bool,
    error: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if error {
        Color::from(INPUT_BORDER_ERROR)
    } else if focused {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if error { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let h_pad = 8.0;
    let inner = Rect::from_xywh(
        rect.left + h_pad,
        rect.top + 2.0,
        rect.width() - 2.0 * h_pad,
        rect.height() - 4.0,
    );
    canvas.save();
    canvas.clip_rect(inner, ClipOp::Intersect, false);

    let (_, metrics) = cache.font.metrics();
    let text_y =
        rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

    if !input.content.is_empty()
        && let Some(blob) = TextBlob::new(&input.content, &cache.font)
    {
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (inner.left, text_y), &paint);
    }
    if focused {
        let cursor_pos = input.cursor.min(input.content.len());
        let cursor_x = if cursor_pos == 0 {
            0.0
        } else {
            let (adv, _) = cache.font.measure_str(&input.content[..cursor_pos], None);
            adv
        };
        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(
                inner.left + cursor_x,
                rect.top + 5.0,
                1.5,
                rect.height() - 10.0,
            ),
            &paint,
        );
    }
    canvas.restore();
}

fn draw_tags_trigger(
    canvas: &Canvas,
    trigger: Rect,
    selected_tags: &[String],
    focused_or_open: bool,
    open: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    let rrect = RRect::new_rect_xy(trigger, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if focused_or_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let chevron_w = 24.0;
    let cx = trigger.right - chevron_w / 2.0;
    let cy = trigger.top + trigger.height() / 2.0;
    let s = 4.0;
    let mut pb = PathBuilder::new();
    if open {
        pb.move_to((cx - s, cy + s * 0.5));
        pb.line_to((cx, cy - s * 0.5));
        pb.line_to((cx + s, cy + s * 0.5));
    } else {
        pb.move_to((cx - s, cy - s * 0.5));
        pb.line_to((cx, cy + s * 0.5));
        pb.line_to((cx + s, cy - s * 0.5));
    }
    paint.set_color(Color::from(PLACEHOLDER_FG));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    canvas.draw_path(&pb.detach(), &paint);
    paint.set_style(PaintStyle::Fill);

    let text_area = Rect::from_xywh(
        trigger.left + 8.0,
        trigger.top,
        trigger.width() - 8.0 - chevron_w,
        trigger.height(),
    );
    canvas.save();
    canvas.clip_rect(text_area, ClipOp::Intersect, false);
    let (_, metrics) = cache.font.metrics();
    let text_y = trigger.top + (trigger.height() - (metrics.descent - metrics.ascent)) / 2.0
        - metrics.ascent;
    if selected_tags.is_empty() {
        if let Some(blob) = TextBlob::new("Select tags…", &cache.font) {
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (text_area.left, text_y), &paint);
        }
    } else {
        let text = selected_tags.join(", ");
        if let Some(blob) = TextBlob::new(&text, &cache.font) {
            paint.set_color(Color::from(INPUT_FG));
            canvas.draw_text_blob(&blob, (text_area.left, text_y), &paint);
        }
    }
    canvas.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_dropdown(
    canvas: &Canvas,
    width: f32,
    height: f32,
    filtered: &[&Tag],
    scroll: usize,
    hovered: Option<usize>,
    filter_input: &TextInput,
    selected_tags: &[TagId],
    cache: &RenderCache,
) {
    let dd = {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        let panel = Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph);
        let x = panel.left + PLAN_FORM_PADDING;
        let w = panel.width() - 2.0 * PLAN_FORM_PADDING;
        let y0 = panel.top + TITLE_H + 1.0 + PLAN_FORM_PADDING;
        let y = y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP;
        let trigger = Rect::from_xywh(x, y, w, PLAN_INPUT_H);
        Rect::from_xywh(
            trigger.left,
            trigger.bottom + 4.0,
            trigger.width(),
            DROPDOWN_H,
        )
    };

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    let h_pad = 8.0;
    let input_h = 28.0;
    let filter_rect = Rect::from_xywh(
        dd.left + h_pad,
        dd.top + (DROPDOWN_FILTER_H - input_h) / 2.0,
        dd.width() - 2.0 * h_pad,
        input_h,
    );
    draw_text_input(canvas, filter_rect, filter_input, true, false, cache);

    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + DROPDOWN_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    let list_top = dd.top + DROPDOWN_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        DROPDOWN_H - DROPDOWN_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if filtered.is_empty() {
        let msg = if filter_input.content.trim().is_empty() {
            "No tags defined yet"
        } else {
            "No matches"
        };
        if let Some(blob) = TextBlob::new(msg, &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            let ty = list_top + 10.0 - sm.ascent;
            paint.set_color(Color::from(PANEL_TEXT));
            canvas.draw_text_blob(&blob, (dd.left + h_pad, ty), &paint);
        }
    } else {
        let (_, metrics) = cache.font.metrics();
        let text_y_off =
            (DROPDOWN_ROW_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;

        let end = (scroll + MAX_DROPDOWN_ROWS).min(filtered.len());
        for (vis_idx, tag) in filtered[scroll..end].iter().enumerate() {
            let abs_idx = scroll + vis_idx;
            let ry = list_top + vis_idx as f32 * DROPDOWN_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), DROPDOWN_ROW_H);

            if hovered == Some(abs_idx) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            let is_selected = selected_tags.contains(&tag.id);
            let circle_cx = dd.left + 18.0;
            let circle_cy = ry + DROPDOWN_ROW_H / 2.0;
            let circle_r = 5.5;
            if is_selected {
                paint.set_color(Color::from(INPUT_BORDER_FOCUS));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_circle((circle_cx, circle_cy), circle_r, &paint);
                paint.set_color(Color::WHITE);
                canvas.draw_circle((circle_cx, circle_cy), 2.0, &paint);
            } else {
                paint.set_color(Color::from(INPUT_BORDER));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.0);
                canvas.draw_circle((circle_cx, circle_cy), circle_r, &paint);
            }
            paint.set_style(PaintStyle::Fill);

            if let Some(blob) = TextBlob::new(&tag.name, &cache.font) {
                paint.set_color(Color::from(ITEM_FG));
                canvas.draw_text_blob(&blob, (dd.left + 34.0, ry + text_y_off), &paint);
            }
        }

        if scroll > 0 {
            paint.set_color(Color::from(MUTED_FG));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            let ax = dd.right - 12.0;
            let ay = list_top + 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to((ax - 4.0, ay + 4.0));
            pb.line_to((ax, ay));
            pb.line_to((ax + 4.0, ay + 4.0));
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
        }
        if end < filtered.len() {
            paint.set_color(Color::from(MUTED_FG));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            let ax = dd.right - 12.0;
            let ay = list_rect.bottom - 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to((ax - 4.0, ay - 4.0));
            pb.line_to((ax, ay));
            pb.line_to((ax + 4.0, ay - 4.0));
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
        }
    }

    canvas.restore();
}

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for UserFormWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let save_btn = Self::save_btn_rect(width, height);

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

        let title = self.title();
        if let Some(blob) = TextBlob::new(title, &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str(title, None);
            let tx = panel.left + (panel.width() - advance) / 2.0;
            let ty =
                panel.top + (TITLE_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        draw_chevron_btn(canvas, back_btn, self.hovered_back);

        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        let y0 = Self::form_top(width, height);
        let lx = panel.left + PLAN_FORM_PADDING;
        let (_, sm_metrics) = cache.small_font.metrics();
        let label_y_offset = -sm_metrics.ascent;

        // Name
        if let Some(blob) = TextBlob::new("Name", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, y0 + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            Self::input_rect(Field::Name, width, height),
            &self.name,
            self.focused == Field::Name,
            self.name_error,
            cache,
        );

        // Tags
        let tags_label_y = y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP;
        if let Some(blob) = TextBlob::new("Tags", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, tags_label_y + label_y_offset), &paint);
        }
        let selected_names: Vec<String> = self
            .selected_tags
            .iter()
            .filter_map(|id| {
                plan.tags
                    .iter()
                    .find(|t| &t.id == id)
                    .map(|t| t.name.clone())
            })
            .collect();
        draw_tags_trigger(
            canvas,
            Self::input_rect(Field::TagFilter, width, height),
            &selected_names,
            self.focused == Field::TagFilter || self.dropdown_open,
            self.dropdown_open,
            cache,
        );

        // Avatar path
        let av_y = y0 + 2.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP);
        if let Some(blob) = TextBlob::new(self.avatar_label(), &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, av_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            Self::avatar_input_rect(width, height),
            &self.avatar_path,
            self.focused == Field::AvatarPath,
            false,
            cache,
        );
        // Browse button
        let browse_btn = Self::browse_btn_rect(width, height);
        paint.set_color(Color::from(if self.hovered_browse {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        }));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(browse_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        if let Some(blob) = TextBlob::new("Browse…", &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            let (adv, _) = cache.small_font.measure_str("Browse…", None);
            let tx = browse_btn.left + (browse_btn.width() - adv) / 2.0;
            let ty =
                browse_btn.top + (browse_btn.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
            paint.set_color(Color::from(BTN_PRIMARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        if self.avatar_error {
            let err_y = Self::input_rect(Field::AvatarPath, width, height).bottom + 4.0;
            if let Some(blob) = TextBlob::new("Could not read file", &cache.small_font) {
                paint.set_color(Color::from(0xff_e53935_u32));
                canvas.draw_text_blob(&blob, (lx, err_y + label_y_offset), &paint);
            }
        }

        // Save button
        paint.set_color(Color::from(if self.hovered_save {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        }));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(save_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        if let Some(blob) = TextBlob::new("Save", &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str("Save", None);
            let tx = save_btn.left + (SAVE_BTN_W - advance) / 2.0;
            let ty = save_btn.top + (PLAN_BTN_H - (metrics.descent - metrics.ascent)) / 2.0
                - metrics.ascent;
            paint.set_color(Color::from(BTN_PRIMARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Dropdown (drawn last so it appears on top)
        if self.dropdown_open {
            let filtered = self.filtered_tags(plan);
            draw_dropdown(
                canvas,
                width,
                height,
                &filtered,
                self.dropdown_scroll,
                self.dropdown_hovered,
                &self.tag_filter,
                &self.selected_tags,
                cache,
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
        let new_save = Self::save_btn_rect(width, height).contains(pt);

        let new_dd_hovered = if self.dropdown_open {
            let dd = Self::dropdown_rect(width, height);
            let list_top = Self::dropdown_list_top(width, height);
            let filtered = self.filtered_tags(plan);
            let visible = filtered
                .len()
                .saturating_sub(self.dropdown_scroll)
                .min(MAX_DROPDOWN_ROWS);
            if x >= dd.left
                && x <= dd.right
                && y >= list_top
                && y < list_top + visible as f32 * DROPDOWN_ROW_H
            {
                let abs_idx = ((y - list_top) / DROPDOWN_ROW_H) as usize + self.dropdown_scroll;
                if abs_idx < filtered.len() {
                    Some(abs_idx)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let new_browse = Self::browse_btn_rect(width, height).contains(pt);
        let changed = new_back != self.hovered_back
            || new_save != self.hovered_save
            || new_browse != self.hovered_browse
            || new_dd_hovered != self.dropdown_hovered;
        if changed {
            self.hovered_back = new_back;
            self.hovered_save = new_save;
            self.hovered_browse = new_browse;
            self.dropdown_hovered = new_dd_hovered;
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
        plan: &Plan,
        sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let pt = Point::new(x, y);

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(pt) {
            return self.try_submit(sender);
        }
        // Browse button opens a native file picker
        if Self::browse_btn_rect(width, height).contains(pt) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                .pick_file()
            {
                self.avatar_path
                    .set_content(path.to_string_lossy().as_ref());
                self.avatar_error = false;
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if self.dropdown_open {
            let dd = Self::dropdown_rect(width, height);
            if dd.contains(pt) {
                let list_top = Self::dropdown_list_top(width, height);
                if y >= list_top {
                    let filtered = self.filtered_tags(plan);
                    let abs_idx = ((y - list_top) / DROPDOWN_ROW_H) as usize + self.dropdown_scroll;
                    if let Some(tag) = filtered.get(abs_idx) {
                        self.toggle_tag(tag.id);
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            self.close_dropdown();
            if !Self::panel_rect(width, height).contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if Self::input_rect(Field::TagFilter, width, height).contains(pt) {
            self.open_dropdown();
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Name field click
        let name_rect = Self::input_rect(Field::Name, width, height);
        if name_rect.contains(pt) {
            self.set_focus(Field::Name);
            let x_in_inner = x - (name_rect.left + 8.0) + self.name.scroll_x.get();
            self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        // Avatar path field click (narrower rect excluding browse button)
        let av_rect = Self::avatar_input_rect(width, height);
        if av_rect.contains(pt) {
            self.set_focus(Field::AvatarPath);
            let x_in_inner = x - (av_rect.left + 8.0) + self.avatar_path.scroll_x.get();
            self.avatar_path.cursor = self.avatar_path.cursor_for_x(x_in_inner, &cache.font);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if !Self::panel_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(&mut self, key: &Key, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) if self.dropdown_open => {
                self.close_dropdown();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) if self.dropdown_open => {
                self.close_dropdown();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Enter) => self.try_submit(sender),
            Key::Named(NamedKey::Tab) => {
                self.cycle_focus_forward();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Backspace) => {
                if self.dropdown_open {
                    self.tag_filter.backspace();
                    self.dropdown_scroll = 0;
                } else if self.focused != Field::TagFilter {
                    self.focused_input().backspace();
                    self.avatar_error = false;
                    if self.focused == Field::Name {
                        self.name_error = false;
                    }
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if self.dropdown_open {
                    self.tag_filter.move_left();
                } else if self.focused != Field::TagFilter {
                    self.focused_input().move_left();
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowRight) => {
                if self.dropdown_open {
                    self.tag_filter.move_right();
                } else if self.focused != Field::TagFilter {
                    self.focused_input().move_right();
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Home) => {
                if self.dropdown_open {
                    self.tag_filter.move_home();
                } else if self.focused != Field::TagFilter {
                    self.focused_input().move_home();
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::End) => {
                if self.dropdown_open {
                    self.tag_filter.move_end();
                } else if self.focused != Field::TagFilter {
                    self.focused_input().move_end();
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Space) => {
                if self.dropdown_open {
                    self.tag_filter.insert_str(" ");
                    self.dropdown_scroll = 0;
                } else if self.focused != Field::TagFilter {
                    self.focused_input().insert_str(" ");
                    self.avatar_error = false;
                    if self.focused == Field::Name {
                        self.name_error = false;
                    }
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    if self.dropdown_open {
                        self.tag_filter.insert_str(c.as_str());
                        self.dropdown_scroll = 0;
                    } else if self.focused == Field::TagFilter {
                        self.open_dropdown();
                        self.tag_filter.insert_str(c.as_str());
                    } else {
                        self.focused_input().insert_str(c.as_str());
                        self.avatar_error = false;
                        if self.focused == Field::Name {
                            self.name_error = false;
                        }
                    }
                    FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        plan: &Plan,
        _width: f32,
        _height: f32,
    ) -> FloatingWindowOutcome {
        if !self.dropdown_open {
            return FloatingWindowOutcome::default();
        }
        let total = self.filtered_tags(plan).len();
        let max_scroll = total.saturating_sub(MAX_DROPDOWN_ROWS);
        if max_scroll == 0 {
            return FloatingWindowOutcome::default();
        }
        let new_scroll = if delta_y > 0.0 {
            self.dropdown_scroll.saturating_sub(1)
        } else {
            (self.dropdown_scroll + 1).min(max_scroll)
        };
        if new_scroll != self.dropdown_scroll {
            self.dropdown_scroll = new_scroll;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
        self.hovered_browse = false;
        self.dropdown_hovered = None;
    }
}
