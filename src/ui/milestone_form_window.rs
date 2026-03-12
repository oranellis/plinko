//! Floating form for creating or editing a milestone.

use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::keyboard::{Key, NamedKey};

use crate::data::{Milestone, MilestoneId, Plan};
use crate::engine::{MilestonePatch, PlanRequest, PlanRequestSender};
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
    + FIELD_BLOCK_H   // description
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;
const SAVE_BTN_W: f32 = 80.0;

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Description,
}

enum Mode {
    New,
    Edit(MilestoneId),
}

pub struct MilestoneFormWindow {
    mode: Mode,
    name: TextInput,
    description: TextInput,
    focused: Field,
    hovered_back: bool,
    hovered_save: bool,
}

impl MilestoneFormWindow {
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            description: TextInput::new(""),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
        }
    }

    pub fn from_milestone(milestone: &Milestone) -> Self {
        let mut name = TextInput::new(&milestone.name);
        name.focused = true;
        Self {
            mode: Mode::Edit(milestone.id),
            name,
            description: TextInput::new(&milestone.description),
            focused: Field::Name,
            hovered_back: false,
            hovered_save: false,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::New => "Add Milestone",
            Mode::Edit(_) => "Edit Milestone",
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
            Field::Description => y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP + LABEL_H + PLAN_LABEL_GAP,
        };
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn set_focus(&mut self, field: Field) {
        self.name.focused = field == Field::Name;
        self.description.focused = field == Field::Description;
        self.focused = field;
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused {
            Field::Name => &mut self.name,
            Field::Description => &mut self.description,
        }
    }

    fn try_submit(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self.name.content.trim().to_string();
        if name.is_empty() {
            return FloatingWindowOutcome::default();
        }
        let description = self.description.content.trim().to_string();
        match self.mode {
            Mode::New => {
                sender.send(PlanRequest::CreateMilestone(Milestone::new(
                    name,
                    description,
                )));
            }
            Mode::Edit(milestone_id) => {
                let patch = MilestonePatch::new().name(name).description(description);
                sender.send(PlanRequest::UpdateMilestone(milestone_id, patch));
            }
        }
        FloatingWindowOutcome::close()
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

    let cursor_pos = input.cursor.min(input.content.len());
    let cursor_x_px = if cursor_pos == 0 {
        0.0f32
    } else {
        cache.font.measure_str(&input.content[..cursor_pos], None).0
    };

    let scroll_x = if focused {
        let inner_w = inner.width();
        let prev = input.scroll_x.get();
        let next = if cursor_x_px < prev {
            cursor_x_px
        } else if cursor_x_px > prev + inner_w {
            cursor_x_px - inner_w + 8.0
        } else {
            prev
        };
        input.scroll_x.set(next);
        next
    } else {
        0.0
    };

    canvas.save();
    canvas.clip_rect(inner, ClipOp::Intersect, false);
    let (_, metrics) = cache.font.metrics();
    let text_y =
        rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
    if !input.content.is_empty()
        && let Some(blob) = TextBlob::new(&input.content, &cache.font)
    {
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (inner.left - scroll_x, text_y), &paint);
    }
    if focused {
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
    }
    canvas.restore();
}

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for MilestoneFormWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, _plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let save_btn = Self::save_btn_rect(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from_argb(40, 0, 0, 0));
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
            cache,
        );

        // Description
        let desc_label_y = y0 + FIELD_BLOCK_H + PLAN_FIELD_GAP;
        if let Some(blob) = TextBlob::new("Description", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, desc_label_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            Self::input_rect(Field::Description, width, height),
            &self.description,
            self.focused == Field::Description,
            cache,
        );

        // Save button
        paint.set_color(Color::from(if self.hovered_save {
            0xff_3a7bc8_u32
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
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _plan: &Plan,
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
        _plan: &Plan,
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
        for field in [Field::Name, Field::Description] {
            let rect = Self::input_rect(field, width, height);
            if rect.contains(pt) {
                self.set_focus(field);
                let inner_left = rect.left + 8.0;
                let x_in_inner = x - inner_left
                    + match field {
                        Field::Name => self.name.scroll_x.get(),
                        Field::Description => self.description.scroll_x.get(),
                    };
                match field {
                    Field::Name => {
                        self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
                    }
                    Field::Description => {
                        self.description.cursor =
                            self.description.cursor_for_x(x_in_inner, &cache.font);
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }
        if !Self::panel_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(&mut self, key: &Key, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => self.try_submit(sender),
            Key::Named(NamedKey::Tab) => {
                let next = match self.focused {
                    Field::Name => Field::Description,
                    Field::Description => Field::Name,
                };
                self.set_focus(next);
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_input().backspace();
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
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    self.focused_input().insert_str(c.as_str());
                    FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
    }
}
