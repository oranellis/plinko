//! Floating form for editing an existing team member.

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::keyboard::{Key, NamedKey};

use crate::data::ids::UserId;
use crate::engine::{PlanRequest, PlanRequestSender, UserPatch};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, DIVIDER_COLOR, INPUT_BG, INPUT_BORDER, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR,
    INPUT_FG, ITEM_FG, LABEL_FG, LIST_BG, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP,
    PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP, TOOLBAR_STROKE_WIDTH,
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
    + FIELD_BLOCK_H   // tags
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // avatar path
    + LABEL_H          // error / hint row
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;
const SAVE_BTN_W: f32 = 80.0;

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Tags,
    AvatarPath,
}

/// Floating form for editing an existing [`User`](crate::data::User).
pub struct EditUserWindow {
    user_id: UserId,
    name: TextInput,
    tags: TextInput,
    avatar_path: TextInput,
    focused: Field,
    hovered_back: bool,
    hovered_save: bool,
    avatar_error: bool,
    pending_back: bool,
}

impl EditUserWindow {
    pub fn new(user: &crate::data::User) -> Self {
        let mut sorted_tags: Vec<&str> = user.tags.iter().map(String::as_str).collect();
        sorted_tags.sort_unstable();
        let tags_str = sorted_tags.join(", ");

        let mut name = TextInput::new(&user.name);
        name.focused = true;

        Self {
            user_id: user.id,
            name,
            tags: TextInput::new(tags_str),
            avatar_path: TextInput::new(""),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
            avatar_error: false,
            pending_back: false,
        }
    }

    fn panel_rect(width: f32, height: f32) -> Rect {
        let x = (width - PANEL_W) / 2.0;
        let y = (height - PANEL_H) / 2.0;
        Rect::from_xywh(x, y, PANEL_W, PANEL_H)
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
        let y = panel.bottom - PLAN_FORM_PADDING - PLAN_BTN_H;
        let x = panel.right - PLAN_FORM_PADDING - SAVE_BTN_W;
        Rect::from_xywh(x, y, SAVE_BTN_W, PLAN_BTN_H)
    }

    fn form_top(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn input_rect(&self, field: Field, width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        let x = panel.left + PLAN_FORM_PADDING;
        let w = PANEL_W - 2.0 * PLAN_FORM_PADDING;
        let y0 = Self::form_top(width, height);
        let y = match field {
            Field::Name => y0 + LABEL_H + PLAN_LABEL_GAP,
            Field::Tags => y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP,
            Field::AvatarPath => {
                y0 + 2.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP) + LABEL_H + PLAN_LABEL_GAP
            }
        };
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused {
            Field::Name => &mut self.name,
            Field::Tags => &mut self.tags,
            Field::AvatarPath => &mut self.avatar_path,
        }
    }

    fn set_focus(&mut self, field: Field) {
        self.name.focused = field == Field::Name;
        self.tags.focused = field == Field::Tags;
        self.avatar_path.focused = field == Field::AvatarPath;
        self.focused = field;
    }

    fn cycle_focus_forward(&mut self) {
        let next = match self.focused {
            Field::Name => Field::Tags,
            Field::Tags => Field::AvatarPath,
            Field::AvatarPath => Field::Name,
        };
        self.set_focus(next);
    }

    fn try_submit(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self.name.content.trim().to_string();

        let avatar: Option<Option<Vec<u8>>> = if self.avatar_path.content.trim().is_empty() {
            None // leave existing avatar unchanged
        } else {
            match std::fs::read(self.avatar_path.content.trim()) {
                Ok(bytes) => Some(Some(bytes)),
                Err(_) => {
                    self.avatar_error = true;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
        };

        let tags: std::collections::HashSet<String> = self
            .tags
            .content
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();

        let mut patch = UserPatch::new().name(name).tags(tags);
        if let Some(av) = avatar {
            patch = patch.avatar(av);
        }

        sender.send(PlanRequest::UpdateUser(self.user_id, patch));
        self.pending_back = true;
        FloatingWindowOutcome::default()
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
    paint.set_stroke_width(1.0);
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
    let line_h = metrics.descent - metrics.ascent;
    let text_y = rect.top + (rect.height() - line_h) / 2.0 - metrics.ascent;

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

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for EditUserWindow {
    fn render(
        &self,
        canvas: &Canvas,
        width: f32,
        height: f32,
        cache: &RenderCache,
        _plan: &crate::data::Plan,
    ) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let save_btn = Self::save_btn_rect(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from_argb(40, 0, 0, 0));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(panel.left + 2.0, panel.top + 4.0, PANEL_W, PANEL_H),
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Title bar background
        let title_rect = Rect::from_xywh(panel.left, panel.top, PANEL_W, TITLE_H);
        paint.set_color(Color::from(LIST_BG));
        canvas.draw_rrect(RRect::new_rect_xy(title_rect, CORNER, CORNER), &paint);
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + CORNER, PANEL_W, TITLE_H - CORNER),
            &paint,
        );

        // Title text
        if let Some(blob) = TextBlob::new("Edit Team Member", &cache.font) {
            let (_, metrics) = cache.font.metrics();
            let (advance, _) = cache.font.measure_str("Edit Team Member", None);
            let tx = panel.left + (PANEL_W - advance) / 2.0;
            let ty =
                panel.top + (TITLE_H - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        draw_chevron_btn(canvas, back_btn, self.hovered_back);

        // Divider
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, PANEL_W, 1.0),
            &paint,
        );

        // ── Form fields ───────────────────────────────────────────────────────

        let y0 = Self::form_top(width, height);
        let lx = panel.left + PLAN_FORM_PADDING;
        let (_, sm_metrics) = cache.small_font.metrics();
        let label_y_offset = -(sm_metrics.ascent);

        // Name
        if let Some(blob) = TextBlob::new("Name", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, y0 + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            self.input_rect(Field::Name, width, height),
            &self.name,
            self.focused == Field::Name,
            cache,
        );

        // Tags
        let tags_y = y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP;
        if let Some(blob) = TextBlob::new("Tags (comma-separated)", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, tags_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            self.input_rect(Field::Tags, width, height),
            &self.tags,
            self.focused == Field::Tags,
            cache,
        );

        // Avatar path
        let av_y = y0 + 2.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP);
        if let Some(blob) =
            TextBlob::new("Avatar image path (blank = keep existing)", &cache.small_font)
        {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, av_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            self.input_rect(Field::AvatarPath, width, height),
            &self.avatar_path,
            self.focused == Field::AvatarPath,
            cache,
        );

        // Error message
        if self.avatar_error {
            let err_y = self.input_rect(Field::AvatarPath, width, height).bottom + 4.0;
            if let Some(blob) = TextBlob::new("Could not read file", &cache.small_font) {
                paint.set_color(Color::from(0xff_e53935u32));
                canvas.draw_text_blob(&blob, (lx, err_y + label_y_offset), &paint);
            }
        }

        // Save button
        paint.set_color(Color::from(if self.hovered_save {
            0xff_3a7bc8u32
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
            let ty = save_btn.top
                + (PLAN_BTN_H - (metrics.descent - metrics.ascent)) / 2.0
                - metrics.ascent;
            paint.set_color(Color::from(BTN_PRIMARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _plan: &crate::data::Plan,
    ) -> FloatingWindowOutcome {
        let pt = Point::new(x, y);
        let new_back = Self::back_btn_rect(width, height).contains(pt);
        let new_save = Self::save_btn_rect(width, height).contains(pt);
        if new_back != self.hovered_back || new_save != self.hovered_save {
            self.hovered_back = new_back;
            self.hovered_save = new_save;
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
        _plan: &crate::data::Plan,
        sender: &PlanRequestSender,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let pt = Point::new(x, y);

        if Self::back_btn_rect(width, height).contains(pt) {
            self.pending_back = true;
            return FloatingWindowOutcome::default();
        }
        if Self::save_btn_rect(width, height).contains(pt) {
            return self.try_submit(sender);
        }
        for field in [Field::Name, Field::Tags, Field::AvatarPath] {
            if self.input_rect(field, width, height).contains(pt) {
                self.set_focus(field);
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }
        if !Self::panel_rect(width, height).contains(pt) {
            self.pending_back = true;
            return FloatingWindowOutcome::default();
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(&mut self, key: &Key, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.pending_back = true;
                FloatingWindowOutcome::default()
            }
            Key::Named(NamedKey::Enter) => self.try_submit(sender),
            Key::Named(NamedKey::Tab) => {
                self.cycle_focus_forward();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_input().backspace();
                self.avatar_error = false;
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.focused_input().move_left();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.focused_input().move_right();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Home) => {
                self.focused_input().move_home();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::End) => {
                self.focused_input().move_end();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Space) => {
                self.focused_input().insert_str(" ");
                self.avatar_error = false;
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    self.focused_input().insert_str(c.as_str());
                    self.avatar_error = false;
                    FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
    }

    fn take_replace_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        if self.pending_back {
            self.pending_back = false;
            Some(Box::new(crate::ui::users_window::UsersWindow::new()))
        } else {
            None
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
    }
}
