//! Floating form for creating or editing a milestone — all milestone fields.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::keyboard::{Key, NamedKey};

use crate::data::constraint::{ConstraintKind, DateConstraint};
use crate::data::{Milestone, MilestoneId, Plan};
use crate::engine::{MilestonePatch, PlanRequest, PlanRequestSender};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, CAL_SELECTED_BG,
    DIVIDER_COLOR, ERROR_BG, INPUT_BG, INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS,
    INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG, LIST_BG, MUTED_FG, OVERLAY_LIGHT,
    OVERLAY_SOFT, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING,
    PLAN_INPUT_H, PLAN_LABEL_GAP, SCROLLBAR_THUMB_COLOR, SUBTLE_BG, SUBTLE_FG,
    TOOLBAR_STROKE_WIDTH,
};
use crate::ui::text_input::TextInput;

// ── Layout constants ──────────────────────────────────────────────────────────

const PANEL_W: f32 = 480.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const LABEL_H: f32 = 14.0;
const FIELD_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H;
const COL_GAP: f32 = 12.0;
const SAVE_BTN_W: f32 = 80.0;
const SCROLLBAR_W: f32 = 4.0;

const ROW_NAME: usize = 0;
const ROW_DESC: usize = 1;
const ROW_CONSTRAINT: usize = 2;

const PANEL_H: f32 = TITLE_H
    + 1.0
    + PLAN_FORM_PADDING
    + FIELD_BLOCK_H   // name
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // description
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // constraint kind + date
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;

// Calendar popup dimensions (mirrors task_form_window)
const CAL_PAD: f32 = 8.0;
const CAL_CELL: f32 = 32.0;
const CAL_W: f32 = CAL_CELL * 7.0 + CAL_PAD * 2.0;
const CAL_HEADER_H: f32 = 28.0;
const CAL_DOW_H: f32 = 20.0;
const CAL_ROW_H: f32 = 26.0;
const CAL_FOOTER_H: f32 = 28.0;
const CAL_H: f32 = CAL_PAD + CAL_HEADER_H + CAL_DOW_H + CAL_ROW_H * 6.0 + CAL_FOOTER_H + CAL_PAD;

// ── Helper types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum TextField {
    Name,
    Description,
}

#[derive(Clone, Copy, PartialEq)]
enum ConstraintSel {
    None,
    Earliest,
    Fixed,
    Latest,
}

impl ConstraintSel {
    fn from_opt(c: Option<DateConstraint>) -> (Self, Option<NaiveDate>) {
        match c {
            None => (Self::None, None),
            Some(dc) => {
                let sel = match dc.kind {
                    ConstraintKind::Earliest => Self::Earliest,
                    ConstraintKind::Fixed => Self::Fixed,
                    ConstraintKind::Latest => Self::Latest,
                };
                (sel, Some(dc.date))
            }
        }
    }

    fn to_constraint(self, date: Option<NaiveDate>) -> Option<DateConstraint> {
        let d = date?;
        match self {
            Self::None => None,
            Self::Earliest => Some(DateConstraint::earliest(d)),
            Self::Fixed => Some(DateConstraint::fixed(d)),
            Self::Latest => Some(DateConstraint::latest(d)),
        }
    }
}

struct CalendarPicker {
    value: Option<NaiveDate>,
    nav_year: i32,
    nav_month: u32,
    hovered_day: Option<u32>,
    hovered_prev_year: bool,
    hovered_prev_month: bool,
    hovered_next_month: bool,
    hovered_next_year: bool,
    hovered_clear: bool,
    hovered_today: bool,
    hovered_trigger: bool,
}

impl CalendarPicker {
    fn new(value: Option<NaiveDate>) -> Self {
        let base = value.unwrap_or_else(|| chrono::Local::now().date_naive());
        Self {
            value,
            nav_year: base.year(),
            nav_month: base.month(),
            hovered_day: None,
            hovered_prev_year: false,
            hovered_prev_month: false,
            hovered_next_month: false,
            hovered_next_year: false,
            hovered_clear: false,
            hovered_today: false,
            hovered_trigger: false,
        }
    }

    fn prev_month(&mut self) {
        if self.nav_month == 1 {
            self.nav_month = 12;
            self.nav_year -= 1;
        } else {
            self.nav_month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.nav_month == 12 {
            self.nav_month = 1;
            self.nav_year += 1;
        } else {
            self.nav_month += 1;
        }
    }

    fn prev_year(&mut self) {
        self.nav_year -= 1;
    }

    fn next_year(&mut self) {
        self.nav_year += 1;
    }

    fn reset_hover(&mut self) {
        self.hovered_day = None;
        self.hovered_prev_year = false;
        self.hovered_prev_month = false;
        self.hovered_next_month = false;
        self.hovered_next_year = false;
        self.hovered_clear = false;
        self.hovered_today = false;
        self.hovered_trigger = false;
    }

    fn display_text(&self) -> String {
        match self.value {
            Some(d) => format!("{}", d.format("%d %b %Y")),
            None => "—".to_string(),
        }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (next.unwrap() - first).num_days() as u32
}

fn first_weekday_offset(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap()
        .weekday()
        .num_days_from_monday()
}

// ── Calendar button rects (free functions, mirror task_form_window) ───────────

fn cal_prev_year_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.left + CAL_PAD,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_prev_month_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.left + CAL_PAD + CAL_HEADER_H + 2.0,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_next_month_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.right - CAL_PAD - 2.0 * CAL_HEADER_H - 2.0,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_next_year_btn(cal: Rect) -> Rect {
    Rect::from_xywh(
        cal.right - CAL_PAD - CAL_HEADER_H,
        cal.top + CAL_PAD,
        CAL_HEADER_H,
        CAL_HEADER_H,
    )
}

fn cal_clear_btn(cal: Rect) -> Rect {
    let w = 48.0;
    let h = 22.0;
    Rect::from_xywh(cal.right - CAL_PAD - w, cal.bottom - CAL_PAD - h, w, h)
}

fn cal_today_btn(cal: Rect) -> Rect {
    let w = 48.0;
    let h = 22.0;
    Rect::from_xywh(cal.left + CAL_PAD, cal.bottom - CAL_PAD - h, w, h)
}

fn cal_day_cell(cal: Rect, day_1_offset: u32, day: u32) -> Rect {
    let grid_idx = day_1_offset + day - 1;
    let col = grid_idx % 7;
    let row = grid_idx / 7;
    let grid_top = cal.top + CAL_PAD + CAL_HEADER_H + CAL_DOW_H;
    Rect::from_xywh(
        cal.left + CAL_PAD + col as f32 * CAL_CELL,
        grid_top + row as f32 * CAL_ROW_H,
        CAL_CELL,
        CAL_ROW_H,
    )
}

fn calendar_popup_rect(trigger: Rect, panel: Rect) -> Rect {
    let below = trigger.bottom + 4.0;
    let above = trigger.top - 4.0 - CAL_H;
    let top = if below + CAL_H <= panel.bottom + 8.0 {
        below
    } else {
        above
    };
    let left = (trigger.left + (trigger.width() - CAL_W) / 2.0)
        .max(panel.left + 4.0)
        .min(panel.right - CAL_W - 4.0);
    Rect::from_xywh(left, top, CAL_W, CAL_H)
}

// ── Mode ──────────────────────────────────────────────────────────────────────

enum Mode {
    New,
    Edit(MilestoneId),
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct MilestoneFormWindow {
    mode: Mode,
    name: TextInput,
    description: TextInput,
    focused: TextField,
    constraint_kind: ConstraintSel,
    hovered_constraint_kind: Option<usize>,
    constraint_date: CalendarPicker,
    calendar_open: bool,
    hovered_back: bool,
    hovered_save: bool,
    name_error: bool,
    constraint_date_error: bool,
    form_scroll_y: f32,
}

impl MilestoneFormWindow {
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            description: TextInput::new(""),
            focused: TextField::Name,
            constraint_kind: ConstraintSel::None,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(None),
            calendar_open: false,
            hovered_back: false,
            hovered_save: false,
            name_error: false,
            constraint_date_error: false,
            form_scroll_y: 0.0,
        }
    }

    pub fn from_milestone(milestone: &Milestone) -> Self {
        let mut name = TextInput::new(&milestone.name);
        name.focused = true;
        let (constraint_kind, constraint_val) = ConstraintSel::from_opt(milestone.constraint);
        Self {
            mode: Mode::Edit(milestone.id),
            name,
            description: TextInput::new(&milestone.description),
            focused: TextField::Name,
            constraint_kind,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(constraint_val),
            calendar_open: false,
            hovered_back: false,
            hovered_save: false,
            name_error: false,
            constraint_date_error: false,
            form_scroll_y: 0.0,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::New => "Add Milestone",
            Mode::Edit(_) => "Edit Milestone",
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

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

    fn effective_scroll(&self, width: f32, height: f32) -> f32 {
        let panel_h = Self::panel_rect(width, height).height();
        self.form_scroll_y.min((PANEL_H - panel_h).max(0.0))
    }

    fn save_btn_rect(width: f32, height: f32) -> Rect {
        let panel = Self::panel_rect(width, height);
        Rect::from_xywh(
            panel.right - PLAN_FORM_PADDING - SAVE_BTN_W,
            panel.top + PANEL_H - PLAN_FORM_PADDING - PLAN_BTN_H,
            SAVE_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn form_top(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn row_label_y(row: usize, width: f32, height: f32) -> f32 {
        Self::form_top(width, height) + row as f32 * (FIELD_BLOCK_H + PLAN_FIELD_GAP)
    }

    fn full_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn col_width(width: f32, height: f32) -> f32 {
        let inner = Self::panel_rect(width, height).width() - 2.0 * PLAN_FORM_PADDING;
        (inner - COL_GAP) / 2.0
    }

    fn left_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = Self::col_width(width, height);
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn right_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let cw = Self::col_width(width, height);
        let x = p.left + PLAN_FORM_PADDING + cw + COL_GAP;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, cw, PLAN_INPUT_H)
    }

    fn constraint_kind_btn_rects(width: f32, height: f32) -> [Rect; 4] {
        let r = Self::left_input_rect(ROW_CONSTRAINT, width, height);
        let bw = r.width() / 4.0;
        std::array::from_fn(|i| Rect::from_xywh(r.left + i as f32 * bw, r.top, bw, r.height()))
    }

    // ── Focus / state ─────────────────────────────────────────────────────────

    fn set_focus(&mut self, field: TextField) {
        self.name.focused = field == TextField::Name;
        self.description.focused = field == TextField::Description;
        self.focused = field;
    }

    fn focused_input(&mut self) -> &mut TextInput {
        match self.focused {
            TextField::Name => &mut self.name,
            TextField::Description => &mut self.description,
        }
    }

    fn close_calendar(&mut self) {
        if self.calendar_open {
            self.constraint_date.reset_hover();
            self.calendar_open = false;
        }
    }

    // ── Submit ────────────────────────────────────────────────────────────────

    fn try_submit(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self.name.content.trim().to_string();
        if name.is_empty() {
            self.name_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        // Constraint date required when a constraint type is selected
        if self.constraint_kind != ConstraintSel::None && self.constraint_date.value.is_none() {
            self.constraint_date_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        self.constraint_date_error = false;
        let description = self.description.content.trim().to_string();
        let constraint = self
            .constraint_kind
            .to_constraint(self.constraint_date.value);
        match self.mode {
            Mode::New => {
                let mut m = Milestone::new(name, description);
                m.constraint = constraint;
                sender.send(PlanRequest::CreateMilestone(m));
            }
            Mode::Edit(milestone_id) => {
                let patch = MilestonePatch::new()
                    .name(name)
                    .description(description)
                    .constraint(constraint);
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

fn draw_date_btn(
    canvas: &Canvas,
    rect: Rect,
    picker: &CalendarPicker,
    is_open: bool,
    disabled: bool,
    error: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(if disabled {
        Color::from(SUBTLE_BG)
    } else if error {
        Color::from(ERROR_BG)
    } else {
        Color::from(INPUT_BG)
    });
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if disabled {
        Color::from(0xff_e0e0e0_u32)
    } else if error {
        Color::from(INPUT_BORDER_ERROR)
    } else if is_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else if picker.hovered_trigger {
        Color::from(MUTED_FG)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if error { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let text = picker.display_text();
    if let Some(blob) = TextBlob::new(&text, &cache.font) {
        let (_, m) = cache.font.metrics();
        let ty = rect.top + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(if disabled {
            Color::from(0xff_cccccc_u32)
        } else if picker.value.is_some() {
            Color::from(INPUT_FG)
        } else {
            Color::from(MUTED_FG)
        });
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Tiny calendar icon
    let icon_cx = rect.right - 16.0;
    let icon_cy = rect.top + rect.height() / 2.0;
    let hs = 5.0;
    paint.set_color(if disabled {
        Color::from(0xff_cccccc_u32)
    } else {
        Color::from(SUBTLE_FG)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.2);
    canvas.draw_rect(
        Rect::from_xywh(icon_cx - hs, icon_cy - hs * 0.8, hs * 2.0, hs * 1.8),
        &paint,
    );
    let mut pb = PathBuilder::new();
    pb.move_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8 - 3.0));
    pb.move_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8 - 3.0));
    canvas.draw_path(&pb.detach(), &paint);
}

fn draw_segmented(
    canvas: &Canvas,
    rects: &[Rect],
    labels: &[&str],
    selected: usize,
    hovered: Option<usize>,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let n = rects.len();
    for (i, (rect, label)) in rects.iter().zip(labels.iter()).enumerate() {
        let is_first = i == 0;
        let is_last = i == n - 1;
        let is_sel = i == selected;
        let is_hov = hovered == Some(i);
        let bg = if is_sel {
            BTN_PRIMARY_BG
        } else if is_hov {
            0xff_e0e0e0_u32
        } else {
            BTN_SECONDARY_BG
        };
        canvas.save();
        canvas.clip_rect(*rect, ClipOp::Intersect, false);
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        let r = if is_first || is_last {
            PLAN_BTN_CORNER
        } else {
            0.0
        };
        canvas.draw_rrect(RRect::new_rect_xy(*rect, r, r), &paint);
        canvas.restore();

        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        if !is_last {
            canvas.draw_line((rect.right, rect.top), (rect.right, rect.bottom), &paint);
        }

        if let Some(blob) = TextBlob::new(label, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(label, None);
            let (_, sm) = cache.small_font.metrics();
            let tx = rect.left + (rect.width() - adv) / 2.0;
            let ty = rect.top + (rect.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
            paint.set_style(PaintStyle::Fill);
            paint.set_color(if is_sel {
                Color::from(BTN_PRIMARY_FG)
            } else {
                Color::from(BTN_SECONDARY_FG)
            });
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }
    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
        let group = Rect::from_ltrb(first.left, first.top, last.right, last.bottom);
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(group, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
    }
}

fn draw_calendar_popup(
    canvas: &Canvas,
    cal: Rect,
    picker: &CalendarPicker,
    today: NaiveDate,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from(OVERLAY_LIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(cal.left + 2.0, cal.top + 4.0, cal.width(), cal.height()),
            CORNER,
            CORNER,
        ),
        &paint,
    );
    paint.set_color(Color::from(PANEL_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_style(PaintStyle::Fill);

    let (_, sm) = cache.small_font.metrics();
    let sm_h = sm.descent - sm.ascent;

    let nav_btns = [
        (cal_prev_year_btn(cal), picker.hovered_prev_year, -2i32),
        (cal_prev_month_btn(cal), picker.hovered_prev_month, -1),
        (cal_next_month_btn(cal), picker.hovered_next_month, 1),
        (cal_next_year_btn(cal), picker.hovered_next_year, 2),
    ];
    for (btn, hov, dir) in nav_btns {
        let bg = if hov {
            0xff_e0e0e0_u32
        } else {
            0xff_f7f7f7_u32
        };
        paint.set_color(Color::from(bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        let cx = btn.left + btn.width() / 2.0;
        let cy = btn.top + btn.height() / 2.0;
        let double = dir.abs() == 2;
        let s = 4.0;
        let offset = if double { 2.5 } else { 0.0 };
        paint.set_color(Color::from(ITEM_FG));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        let double_shifts = [-offset, offset];
        let single_shift = [0.0f32];
        let shifts: &[f32] = if double {
            &double_shifts
        } else {
            &single_shift
        };
        for shift in shifts {
            let ox = if dir < 0 { *shift } else { -*shift };
            let mut pb = PathBuilder::new();
            if dir < 0 {
                pb.move_to((cx + ox + s * 0.45, cy - s));
                pb.line_to((cx + ox - s * 0.45, cy));
                pb.line_to((cx + ox + s * 0.45, cy + s));
            } else {
                pb.move_to((cx + ox - s * 0.45, cy - s));
                pb.line_to((cx + ox + s * 0.45, cy));
                pb.line_to((cx + ox - s * 0.45, cy + s));
            }
            canvas.draw_path(&pb.detach(), &paint);
        }
        paint.set_style(PaintStyle::Fill);
    }

    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_str = format!(
        "{} {}",
        month_names[(picker.nav_month as usize).saturating_sub(1).min(11)],
        picker.nav_year
    );
    if let Some(blob) = TextBlob::new(&month_str, &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str(&month_str, None);
        let tx = cal.left + (cal.width() - adv) / 2.0;
        let ty = cal_prev_year_btn(cal).top + (CAL_HEADER_H - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(ITEM_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    let dow_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let dow_y_top = cal.top + CAL_PAD + CAL_HEADER_H;
    for (i, lbl) in dow_labels.iter().enumerate() {
        let cx = cal.left + CAL_PAD + i as f32 * CAL_CELL + CAL_CELL / 2.0;
        if let Some(blob) = TextBlob::new(lbl, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(lbl, None);
            let ty = dow_y_top + (CAL_DOW_H - sm_h) / 2.0 - sm.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (cx - adv / 2.0, ty), &paint);
        }
    }

    let day_1_offset = first_weekday_offset(picker.nav_year, picker.nav_month);
    let num_days = days_in_month(picker.nav_year, picker.nav_month);
    let today_in_nav = today.year() == picker.nav_year && today.month() == picker.nav_month;

    for day in 1..=num_days {
        let cell = cal_day_cell(cal, day_1_offset, day);
        let is_selected = picker
            .value
            .map(|v| v.year() == picker.nav_year && v.month() == picker.nav_month && v.day() == day)
            .unwrap_or(false);
        let is_today = today_in_nav && today.day() == day;
        let is_hov = picker.hovered_day == Some(day);
        let cx = cell.left + cell.width() / 2.0;
        let cy = cell.top + cell.height() / 2.0;

        if is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        } else if is_hov {
            paint.set_color(Color::from(CAL_SELECTED_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        }

        let day_str = format!("{}", day);
        if let Some(blob) = TextBlob::new(&day_str, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(&day_str, None);
            let tx = cx - adv / 2.0;
            let ty = cell.top + (cell.height() - sm_h) / 2.0 - sm.ascent;
            paint.set_color(if is_selected {
                Color::WHITE
            } else if is_today {
                Color::from(BTN_PRIMARY_BG)
            } else {
                Color::from(ITEM_FG)
            });
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        if is_today && !is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cell.top + cell.height() - 4.0), 2.0, &paint);
        }
    }

    let footer_y = cal.bottom - CAL_PAD - CAL_FOOTER_H;
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(cal.left, footer_y, cal.width(), 1.0),
        &paint,
    );

    let clear_btn = cal_clear_btn(cal);
    paint.set_color(Color::from(if picker.hovered_clear {
        0xff_e0e0e0_u32
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(clear_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Clear", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Clear", None);
        let tx = clear_btn.left + (clear_btn.width() - adv) / 2.0;
        let ty = clear_btn.top + (clear_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    let today_btn = cal_today_btn(cal);
    paint.set_color(Color::from(if picker.hovered_today {
        0xff_e0e0e0_u32
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(today_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Today", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Today", None);
        let tx = today_btn.left + (today_btn.width() - adv) / 2.0;
        let ty = today_btn.top + (today_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }
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

        let scroll_y = self.effective_scroll(width, height);

        // Clip content to below title bar and apply vertical scroll
        let content_clip = Rect::from_xywh(
            panel.left,
            panel.top + TITLE_H + 1.0,
            panel.width(),
            panel.height() - TITLE_H - 1.0,
        );
        canvas.save();
        canvas.clip_rect(content_clip, ClipOp::Intersect, false);
        canvas.translate((0.0, -scroll_y));

        let lx = panel.left + PLAN_FORM_PADDING;
        let (_, sm_metrics) = cache.small_font.metrics();
        let label_y_offset = -sm_metrics.ascent;

        // Name
        let name_label_y = Self::row_label_y(ROW_NAME, width, height);
        if let Some(blob) = TextBlob::new("Name", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, name_label_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            Self::full_input_rect(ROW_NAME, width, height),
            &self.name,
            self.focused == TextField::Name,
            self.name_error,
            cache,
        );

        // Description
        let desc_label_y = Self::row_label_y(ROW_DESC, width, height);
        if let Some(blob) = TextBlob::new("Description", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, desc_label_y + label_y_offset), &paint);
        }
        draw_text_input(
            canvas,
            Self::full_input_rect(ROW_DESC, width, height),
            &self.description,
            self.focused == TextField::Description,
            false,
            cache,
        );

        // Constraint row: kind segmented (left) + date button (right)
        let con_label_y = Self::row_label_y(ROW_CONSTRAINT, width, height);
        if let Some(blob) = TextBlob::new("Constraint", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, con_label_y + label_y_offset), &paint);
        }
        let ck_sel = match self.constraint_kind {
            ConstraintSel::None => 0,
            ConstraintSel::Earliest => 1,
            ConstraintSel::Fixed => 2,
            ConstraintSel::Latest => 3,
        };
        draw_segmented(
            canvas,
            &Self::constraint_kind_btn_rects(width, height),
            &["None", "Earliest", "Fixed", "Latest"],
            ck_sel,
            self.hovered_constraint_kind,
            cache,
        );
        let date_disabled = self.constraint_kind == ConstraintSel::None;
        draw_date_btn(
            canvas,
            Self::right_input_rect(ROW_CONSTRAINT, width, height),
            &self.constraint_date,
            self.calendar_open,
            date_disabled,
            self.constraint_date_error,
            cache,
        );

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

        canvas.restore(); // end content scroll region

        // Scrollbar
        let content_area_h = panel.height() - TITLE_H - 1.0;
        let full_content_h = PANEL_H - TITLE_H - 1.0;
        let max_scroll = (full_content_h - content_area_h).max(0.0);
        if max_scroll > 0.0 {
            let thumb_h = (content_area_h * content_area_h / full_content_h).max(20.0);
            let thumb_y =
                (panel.top + TITLE_H + 1.0) + (scroll_y / max_scroll) * (content_area_h - thumb_h);
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

        // Calendar popup (on top, not clipped by content scroll)
        if self.calendar_open && self.constraint_kind != ConstraintSel::None {
            let trigger = Rect::from_xywh(
                Self::right_input_rect(ROW_CONSTRAINT, width, height).left,
                Self::right_input_rect(ROW_CONSTRAINT, width, height).top - scroll_y,
                Self::right_input_rect(ROW_CONSTRAINT, width, height).width(),
                Self::right_input_rect(ROW_CONSTRAINT, width, height).height(),
            );
            let cal = calendar_popup_rect(trigger, panel);
            let today = chrono::Local::now().date_naive();
            draw_calendar_popup(canvas, cal, &self.constraint_date, today, cache);
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
        let scroll_y = self.effective_scroll(width, height);
        let pt_form = Point::new(x, y + scroll_y);
        let mut changed = false;

        macro_rules! set {
            ($field:expr, $val:expr) => {
                if $field != $val {
                    $field = $val;
                    changed = true;
                }
            };
        }

        if self.calendar_open {
            let trigger_base = Self::right_input_rect(ROW_CONSTRAINT, width, height);
            let trigger = Rect::from_xywh(
                trigger_base.left,
                trigger_base.top - scroll_y,
                trigger_base.width(),
                trigger_base.height(),
            );
            let panel = Self::panel_rect(width, height);
            let cal = calendar_popup_rect(trigger, panel);

            let new_prev_year = cal_prev_year_btn(cal).contains(pt);
            let new_prev_month = cal_prev_month_btn(cal).contains(pt);
            let new_next_month = cal_next_month_btn(cal).contains(pt);
            let new_next_year = cal_next_year_btn(cal).contains(pt);
            let new_clear = cal_clear_btn(cal).contains(pt);
            let new_today = cal_today_btn(cal).contains(pt);
            set!(self.constraint_date.hovered_prev_year, new_prev_year);
            set!(self.constraint_date.hovered_prev_month, new_prev_month);
            set!(self.constraint_date.hovered_next_month, new_next_month);
            set!(self.constraint_date.hovered_next_year, new_next_year);
            set!(self.constraint_date.hovered_clear, new_clear);
            set!(self.constraint_date.hovered_today, new_today);

            let day_1 = first_weekday_offset(
                self.constraint_date.nav_year,
                self.constraint_date.nav_month,
            );
            let num_days = days_in_month(
                self.constraint_date.nav_year,
                self.constraint_date.nav_month,
            );
            let mut new_day = None;
            for day in 1..=num_days {
                if cal_day_cell(cal, day_1, day).contains(pt) {
                    new_day = Some(day);
                    break;
                }
            }
            set!(self.constraint_date.hovered_day, new_day);
        } else {
            let new_back = Self::back_btn_rect(width, height).contains(pt);
            let new_save = Self::save_btn_rect(width, height).contains(pt_form);
            set!(self.hovered_back, new_back);
            set!(self.hovered_save, new_save);

            let new_ck = Self::constraint_kind_btn_rects(width, height)
                .iter()
                .position(|r| r.contains(pt_form));
            set!(self.hovered_constraint_kind, new_ck);

            let new_ct = self.constraint_kind != ConstraintSel::None
                && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form);
            set!(self.constraint_date.hovered_trigger, new_ct);
        }

        if changed {
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
        let scroll_y = self.effective_scroll(width, height);
        let pt_form = Point::new(x, y + scroll_y);

        // Calendar popup interactions
        if self.calendar_open {
            let trigger_base = Self::right_input_rect(ROW_CONSTRAINT, width, height);
            let trigger = Rect::from_xywh(
                trigger_base.left,
                trigger_base.top - scroll_y,
                trigger_base.width(),
                trigger_base.height(),
            );
            let panel = Self::panel_rect(width, height);
            let cal = calendar_popup_rect(trigger, panel);
            if cal.contains(pt) {
                if cal_prev_year_btn(cal).contains(pt) {
                    self.constraint_date.prev_year();
                } else if cal_prev_month_btn(cal).contains(pt) {
                    self.constraint_date.prev_month();
                } else if cal_next_month_btn(cal).contains(pt) {
                    self.constraint_date.next_month();
                } else if cal_next_year_btn(cal).contains(pt) {
                    self.constraint_date.next_year();
                } else if cal_clear_btn(cal).contains(pt) {
                    self.constraint_date.value = None;
                    self.close_calendar();
                } else if cal_today_btn(cal).contains(pt) {
                    self.constraint_date.value = Some(chrono::Local::now().date_naive());
                    self.constraint_date_error = false;
                    self.close_calendar();
                } else {
                    let day_1 = first_weekday_offset(
                        self.constraint_date.nav_year,
                        self.constraint_date.nav_month,
                    );
                    let num_days = days_in_month(
                        self.constraint_date.nav_year,
                        self.constraint_date.nav_month,
                    );
                    for day in 1..=num_days {
                        if cal_day_cell(cal, day_1, day).contains(pt) {
                            self.constraint_date.value = NaiveDate::from_ymd_opt(
                                self.constraint_date.nav_year,
                                self.constraint_date.nav_month,
                                day,
                            );
                            self.constraint_date_error = false;
                            self.close_calendar();
                            break;
                        }
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            self.close_calendar();
            if !Self::panel_rect(width, height).contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(pt_form) {
            return self.try_submit(sender);
        }

        // Constraint kind segmented
        for (i, r) in Self::constraint_kind_btn_rects(width, height)
            .iter()
            .enumerate()
        {
            if r.contains(pt_form) {
                let new_kind = match i {
                    0 => ConstraintSel::None,
                    1 => ConstraintSel::Earliest,
                    2 => ConstraintSel::Fixed,
                    _ => ConstraintSel::Latest,
                };
                self.constraint_kind = new_kind;
                if new_kind == ConstraintSel::None {
                    self.close_calendar();
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        // Date trigger button
        if self.constraint_kind != ConstraintSel::None
            && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form)
        {
            self.calendar_open = !self.calendar_open;
            if !self.calendar_open {
                self.constraint_date.reset_hover();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Text inputs
        for field in [TextField::Name, TextField::Description] {
            let rect = Self::full_input_rect(
                match field {
                    TextField::Name => ROW_NAME,
                    TextField::Description => ROW_DESC,
                },
                width,
                height,
            );
            if rect.contains(pt_form) {
                self.set_focus(field);
                let x_in_inner = x - (rect.left + 8.0)
                    + match field {
                        TextField::Name => self.name.scroll_x.get(),
                        TextField::Description => self.description.scroll_x.get(),
                    };
                match field {
                    TextField::Name => {
                        self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
                    }
                    TextField::Description => {
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
        if self.calendar_open {
            if *key == Key::Named(NamedKey::Escape) {
                self.close_calendar();
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }
        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => self.try_submit(sender),
            Key::Named(NamedKey::Tab) => {
                let next = match self.focused {
                    TextField::Name => TextField::Description,
                    TextField::Description => TextField::Name,
                };
                self.set_focus(next);
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_input().backspace();
                if self.focused == TextField::Name {
                    self.name_error = false;
                }
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
                if self.focused == TextField::Name {
                    self.name_error = false;
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    self.focused_input().insert_str(c.as_str());
                    if self.focused == TextField::Name {
                        self.name_error = false;
                    }
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
        self.hovered_constraint_kind = None;
        self.constraint_date.hovered_trigger = false;
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        _plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        let panel_h = Self::panel_rect(width, height).height();
        let max_scroll = (PANEL_H - panel_h).max(0.0);
        if max_scroll <= 0.0 {
            return FloatingWindowOutcome::default();
        }
        let new_scroll = (self.form_scroll_y - delta_y * 40.0).clamp(0.0, max_scroll);
        if (new_scroll - self.form_scroll_y).abs() > f32::EPSILON {
            self.form_scroll_y = new_scroll;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }
}
