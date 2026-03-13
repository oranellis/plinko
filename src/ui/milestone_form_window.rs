//! Floating form for creating or editing a milestone — all milestone fields.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::data::constraint::{ConstraintKind, DateConstraint};
use crate::data::dependency::Dependency;
use crate::data::ids::NodeId;
use crate::data::{Milestone, MilestoneId, Plan};
use crate::engine::{MilestonePatch, PlanRequest, PlanRequestSender, apply_milestone_patch};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, BTN_DANGER_BG, BTN_PRIMARY_BG, BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG,
    BTN_SECONDARY_BG, BTN_SECONDARY_FG, CAL_SELECTED_BG, DEP_PLAN_START_FG, DIVIDER_COLOR,
    ERROR_BG, GHOST_FG, ICON_DELETE_COLOR, INPUT_BG, INPUT_BORDER, INPUT_BORDER_ERROR,
    INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG, LINK_COLOR, LIST_BG,
    LIST_ITEM_HOVER_BG, MUTED_FG, OVERLAY_DARK, OVERLAY_LIGHT, OVERLAY_SOFT, OVERLAY_XLIGHT,
    PANEL_BG, PLACEHOLDER_FG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP, PLAN_FORM_PADDING,
    PLAN_INPUT_H, PLAN_LABEL_GAP, SCROLLBAR_THUMB_COLOR, SUBTLE_BG, SUBTLE_FG,
};
use crate::ui::multi_line_input::MultiLineInput;
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

const DEP_ROW_H: f32 = 36.0;
const DEP_INPUT_H: f32 = 28.0;
const DEP_LAG_W: f32 = 64.0;
const DEP_REMOVE_SIZE: f32 = 22.0;
const DEP_COL_GAP: f32 = 8.0;
const DEP_PAD_L: f32 = 4.0;
const DEP_PAD_R: f32 = 8.0;
const MAX_VISIBLE_DEPS: usize = 3;
const PLUS_BTN_H: f32 = 28.0;
const DEP_SECTION_H: f32 =
    LABEL_H + PLAN_LABEL_GAP + DEP_ROW_H * MAX_VISIBLE_DEPS as f32 + PLUS_BTN_H;

const DEP_DROPDOWN_FILTER_H: f32 = DEP_INPUT_H;
const DEP_DROPDOWN_ROW_H: f32 = 28.0;
const MAX_DEP_DROPDOWN_ROWS: usize = 5;
const DEP_DROPDOWN_H: f32 =
    DEP_DROPDOWN_FILTER_H + MAX_DEP_DROPDOWN_ROWS as f32 * DEP_DROPDOWN_ROW_H;

// Multi-line description box
const DESC_LINE_H: f32 = 18.0;
const DESC_LINES: usize = 8;
const DESC_H: f32 = DESC_LINE_H * DESC_LINES as f32 + 8.0;
const DESC_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + DESC_H;

const ROW_NAME: usize = 0;
const ROW_DESC: usize = 1;
const ROW_CONSTRAINT: usize = 2;

const PANEL_H: f32 = TITLE_H
    + 1.0
    + PLAN_FORM_PADDING
    + FIELD_BLOCK_H   // name
    + PLAN_FIELD_GAP
    + DESC_BLOCK_H    // description (tall)
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // constraint kind + date
    + PLAN_FIELD_GAP
    + DEP_SECTION_H   // dependencies
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

// ── DependencyEdit ────────────────────────────────────────────────────────────

struct DependencyEdit {
    target: Option<NodeId>,
    dep_filter: TextInput,
    lag_input: TextInput,
    hovered_target: bool,
    hovered_remove: bool,
}

impl DependencyEdit {
    fn new() -> Self {
        Self {
            target: None,
            dep_filter: TextInput::new(""),
            lag_input: TextInput::new(""),
            hovered_target: false,
            hovered_remove: false,
        }
    }
}

// ── Mode ──────────────────────────────────────────────────────────────────────

enum Mode {
    New,
    Edit(MilestoneId),
}

// ── Text utilities ────────────────────────────────────────────────────────────

fn wrap_text(text: &str, font: &skia_safe::Font, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        let (w, _) = font.measure_str(&candidate, None);
        if w > max_w && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct MilestoneFormWindow {
    mode: Mode,
    name: TextInput,
    description: MultiLineInput,
    focused: TextField,
    constraint_kind: ConstraintSel,
    hovered_constraint_kind: Option<usize>,
    constraint_date: CalendarPicker,
    calendar_open: bool,
    hovered_back: bool,
    hovered_save: bool,
    name_error: bool,
    constraint_date_error: bool,
    cursor_in_desc: bool,
    dependencies: Vec<DependencyEdit>,
    dep_scroll_y: f32,
    cursor_in_dep_list: bool,
    dep_dropdown_open_for: Option<usize>,
    dep_dropdown_hovered: Option<usize>,
    dep_dropdown_scroll: usize,
    focused_dep_lag: Option<usize>,
    hovered_dep_plus: bool,
    dep_error: bool,
    form_scroll_y: f32,
    scheduler_error: Option<String>,
}

impl MilestoneFormWindow {
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            description: MultiLineInput::new(""),
            focused: TextField::Name,
            constraint_kind: ConstraintSel::None,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(None),
            calendar_open: false,
            hovered_back: false,
            hovered_save: false,
            name_error: false,
            constraint_date_error: false,
            cursor_in_desc: false,
            dependencies: vec![DependencyEdit {
                target: Some(NodeId::PlanStart),
                ..DependencyEdit::new()
            }],
            dep_scroll_y: 0.0,
            cursor_in_dep_list: false,
            dep_dropdown_open_for: None,
            dep_dropdown_hovered: None,
            dep_dropdown_scroll: 0,
            focused_dep_lag: None,
            hovered_dep_plus: false,
            dep_error: false,
            form_scroll_y: 0.0,
            scheduler_error: None,
        }
    }

    pub fn from_milestone(milestone: &Milestone) -> Self {
        let mut name = TextInput::new(&milestone.name);
        name.focused = true;
        let (constraint_kind, constraint_val) = ConstraintSel::from_opt(milestone.constraint);
        Self {
            mode: Mode::Edit(milestone.id),
            name,
            description: MultiLineInput::new(&milestone.description),
            focused: TextField::Name,
            constraint_kind,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(constraint_val),
            calendar_open: false,
            hovered_back: false,
            hovered_save: false,
            name_error: false,
            constraint_date_error: false,
            cursor_in_desc: false,
            dependencies: milestone
                .dependencies
                .iter()
                .map(|d| {
                    let lag_str = if d.lag_days != 0.0 {
                        format!("{}", d.lag_days)
                    } else {
                        String::new()
                    };
                    DependencyEdit {
                        target: Some(d.id),
                        dep_filter: TextInput::new(""),
                        lag_input: TextInput::new(lag_str),
                        hovered_target: false,
                        hovered_remove: false,
                    }
                })
                .collect(),
            dep_scroll_y: 0.0,
            cursor_in_dep_list: false,
            dep_dropdown_open_for: None,
            dep_dropdown_hovered: None,
            dep_dropdown_scroll: 0,
            focused_dep_lag: None,
            hovered_dep_plus: false,
            dep_error: false,
            form_scroll_y: 0.0,
            scheduler_error: None,
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
        let base = Self::form_top(width, height) + row as f32 * (FIELD_BLOCK_H + PLAN_FIELD_GAP);
        if row > ROW_DESC {
            base + (DESC_BLOCK_H - FIELD_BLOCK_H)
        } else {
            base
        }
    }

    fn full_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        let h = if row == ROW_DESC {
            DESC_H
        } else {
            PLAN_INPUT_H
        };
        Rect::from_xywh(x, y, w, h)
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

    fn dep_label_y(width: f32, height: f32) -> f32 {
        Self::row_label_y(ROW_CONSTRAINT, width, height) + FIELD_BLOCK_H + PLAN_FIELD_GAP
    }

    fn dep_list_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::dep_label_y(width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, DEP_ROW_H * MAX_VISIBLE_DEPS as f32)
    }

    fn dep_plus_rect(width: f32, height: f32) -> Rect {
        let list = Self::dep_list_rect(width, height);
        Rect::from_xywh(list.left, list.bottom, list.width(), PLUS_BTN_H)
    }

    fn dep_target_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * DEP_ROW_H;
        let vy = row_y + (DEP_ROW_H - DEP_INPUT_H) / 2.0;
        let w = list.width()
            - DEP_PAD_L
            - DEP_COL_GAP
            - DEP_LAG_W
            - DEP_COL_GAP
            - DEP_REMOVE_SIZE
            - DEP_PAD_R;
        Rect::from_xywh(list.left + DEP_PAD_L, vy, w, DEP_INPUT_H)
    }

    fn dep_lag_rect(list: Rect, abs_idx: usize) -> Rect {
        let target = Self::dep_target_rect(list, abs_idx);
        Rect::from_xywh(
            target.right + DEP_COL_GAP,
            target.top,
            DEP_LAG_W,
            DEP_INPUT_H,
        )
    }

    fn dep_remove_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * DEP_ROW_H;
        Rect::from_xywh(
            list.right - DEP_PAD_R - DEP_REMOVE_SIZE,
            row_y + (DEP_ROW_H - DEP_REMOVE_SIZE) / 2.0,
            DEP_REMOVE_SIZE,
            DEP_REMOVE_SIZE,
        )
    }

    fn dep_dropdown_rect(list: Rect, abs_idx: usize, panel: Rect) -> Rect {
        let target = Self::dep_target_rect(list, abs_idx);
        let below = target.bottom + 2.0;
        let above = target.top - 2.0 - DEP_DROPDOWN_H;
        let top = if below + DEP_DROPDOWN_H <= panel.bottom + 8.0 {
            below
        } else {
            above
        };
        Rect::from_xywh(target.left, top, target.width(), DEP_DROPDOWN_H)
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
            TextField::Description => {
                unreachable!("description is MultiLineInput; handled separately")
            }
        }
    }

    fn close_calendar(&mut self) {
        if self.calendar_open {
            self.constraint_date.reset_hover();
            self.calendar_open = false;
        }
    }

    fn mode_milestone_id(&self) -> Option<MilestoneId> {
        match self.mode {
            Mode::Edit(id) => Some(id),
            Mode::New => None,
        }
    }

    fn open_dep_dropdown(&mut self, dep_idx: usize) {
        self.close_calendar();
        self.dep_dropdown_open_for = Some(dep_idx);
        self.dep_dropdown_hovered = None;
        self.dep_dropdown_scroll = 0;
        self.dependencies[dep_idx].dep_filter = TextInput::new("");
        self.focused_dep_lag = None;
        self.name.focused = false;
        self.description.focused = false;
    }

    fn close_dep_dropdown(&mut self) {
        if let Some(i) = self.dep_dropdown_open_for.take()
            && i < self.dependencies.len()
        {
            self.dependencies[i].dep_filter = TextInput::new("");
        }
        self.dep_dropdown_hovered = None;
    }

    fn clamp_dep_scroll_y(&mut self) {
        let content_h = self.dependencies.len() as f32 * DEP_ROW_H;
        let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
        let max = (content_h - visible_h).max(0.0);
        self.dep_scroll_y = self.dep_scroll_y.clamp(0.0, max);
    }

    // ── Submit ────────────────────────────────────────────────────────────────

    fn try_submit(&mut self, plan: &Plan, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        // Validate all fields at once so every problem is highlighted together.
        let name = self.name.content.trim().to_string();
        self.name_error = name.is_empty();

        self.constraint_date_error =
            self.constraint_kind != ConstraintSel::None && self.constraint_date.value.is_none();

        let dependencies: Vec<Dependency> = self
            .dependencies
            .iter()
            .filter_map(|d| {
                let id = d.target?;
                let lag_days = d.lag_input.content.trim().parse::<f32>().unwrap_or(0.0);
                Some(Dependency { id, lag_days })
            })
            .collect();
        self.dep_error = dependencies.is_empty();

        if self.name_error || self.constraint_date_error || self.dep_error {
            self.scheduler_error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        let description = self.description.content.trim().to_string();
        let constraint = self
            .constraint_kind
            .to_constraint(self.constraint_date.value);

        // Dry-run: clone the plan, apply the mutation, run the scheduler.
        // Only send the real request if the scheduler succeeds.
        let mut dry_plan = plan.clone();
        let sched_result: Result<(), String> = match self.mode {
            Mode::New => {
                let mut m = Milestone::new(name.clone(), description.clone());
                m.constraint = constraint;
                m.dependencies = dependencies.clone();
                dry_plan.add_milestone(m);
                dry_plan
                    .compute_time_optimised_plan()
                    .map_err(|e| e.to_string())
            }
            Mode::Edit(id) => {
                let patch = MilestonePatch::new()
                    .name(name.clone())
                    .description(description.clone())
                    .constraint(constraint)
                    .dependencies(dependencies.clone());
                apply_milestone_patch(&mut dry_plan, id, patch)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        dry_plan
                            .compute_time_optimised_plan()
                            .map_err(|e| e.to_string())
                    })
            }
        };

        if let Err(e) = sched_result {
            self.scheduler_error = Some(e);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        self.scheduler_error = None;

        match self.mode {
            Mode::New => {
                let mut m = Milestone::new(name, description);
                m.constraint = constraint;
                m.dependencies = dependencies;
                sender.send(PlanRequest::CreateMilestone(m));
            }
            Mode::Edit(milestone_id) => {
                let patch = MilestonePatch::new()
                    .name(name)
                    .description(description)
                    .constraint(constraint)
                    .dependencies(dependencies);
                sender.send(PlanRequest::UpdateMilestone(milestone_id, patch));
            }
        }
        FloatingWindowOutcome::close()
    }
}

// ── Drawing helpers ───────────────────────────────────────────────────────────

fn draw_multi_line_input(
    canvas: &Canvas,
    rect: Rect,
    input: &MultiLineInput,
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
    paint.set_stroke_width(if focused { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let (_, metrics) = cache.font.metrics();
    let line_h = metrics.descent - metrics.ascent + 2.0;
    let text_x = rect.left + 8.0;
    let text_top = rect.top + 4.0;
    let inner_width = rect.width() - 16.0;
    let visible_h = rect.height() - 8.0;

    // Clamp scroll (never auto-scroll; that happens only in event handlers).
    input.clamp_scroll(inner_width, &cache.font, line_h, visible_h);
    let scroll_y = input.scroll_y.get();

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            rect.left + 1.0,
            rect.top + 1.0,
            rect.width() - 2.0,
            rect.height() - 2.0,
        ),
        ClipOp::Intersect,
        false,
    );

    let content = &input.content;
    let lines = input.visual_lines(inner_width, &cache.font);
    let link_ranges = MultiLineInput::find_links(content);

    if lines.is_empty() || (lines.len() == 1 && lines[0].text.is_empty() && content.is_empty()) {
        if let Some(blob) = TextBlob::new("Description…", &cache.font) {
            paint.set_color(Color::from(PLACEHOLDER_FG));
            canvas.draw_text_blob(&blob, (text_x, text_top - metrics.ascent), &paint);
        }
    } else {
        for (i, vline) in lines.iter().enumerate() {
            let y = text_top + i as f32 * line_h - scroll_y;
            if y + line_h < rect.top || y > rect.bottom() {
                continue;
            }
            if vline.text.is_empty() {
                continue;
            }

            let line_byte_start = vline.byte_start;
            let line_byte_end = line_byte_start + vline.text.len();

            let mut spans: Vec<(usize, usize, bool)> = Vec::new();
            let mut pos = line_byte_start;
            for range in &link_ranges {
                let link_start = range.start.max(line_byte_start).min(line_byte_end);
                let link_end = range.end.max(line_byte_start).min(line_byte_end);
                if link_start >= link_end {
                    continue;
                }
                if pos < link_start {
                    spans.push((pos, link_start, false));
                }
                spans.push((link_start, link_end, true));
                pos = link_end;
            }
            if pos < line_byte_end {
                spans.push((pos, line_byte_end, false));
            }

            if spans.is_empty() {
                if let Some(blob) = TextBlob::new(vline.text, &cache.font) {
                    paint.set_color(Color::from(INPUT_FG));
                    canvas.draw_text_blob(&blob, (text_x, y - metrics.ascent), &paint);
                }
            } else {
                let mut span_x = text_x;
                for (s_start, s_end, is_link) in spans {
                    let span_text = &content[s_start..s_end];
                    if span_text.is_empty() {
                        continue;
                    }
                    let span_w = cache.font.measure_str(span_text, None).0;
                    if let Some(blob) = TextBlob::new(span_text, &cache.font) {
                        paint.set_color(Color::from(if is_link { LINK_COLOR } else { INPUT_FG }));
                        canvas.draw_text_blob(&blob, (span_x, y - metrics.ascent), &paint);
                        if is_link {
                            paint.set_style(PaintStyle::Stroke);
                            paint.set_stroke_width(1.0);
                            let underline_y = y - metrics.ascent + metrics.descent + 1.0;
                            canvas.draw_line(
                                (span_x, underline_y),
                                (span_x + span_w, underline_y),
                                &paint,
                            );
                            paint.set_style(PaintStyle::Fill);
                        }
                    }
                    span_x += span_w;
                }
            }
        }
    }

    // Cursor
    if focused {
        let cursor_pos = input.clamped_cursor();
        let (cursor_line_idx, cursor_col_str) = {
            let before = &content[..cursor_pos];
            let idx = lines
                .iter()
                .rposition(|l| l.byte_start <= cursor_pos)
                .unwrap_or(0);
            let col_start = lines.get(idx).map(|l| l.byte_start).unwrap_or(0);
            let col_str = before[col_start..].to_owned();
            (idx, col_str)
        };
        let cursor_x = text_x + cache.font.measure_str(&cursor_col_str, None).0;
        let cursor_y_top = text_top + cursor_line_idx as f32 * line_h - scroll_y;
        let cursor_y_bot = cursor_y_top + line_h;
        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_line((cursor_x, cursor_y_top), (cursor_x, cursor_y_bot), &paint);
        paint.set_style(PaintStyle::Fill);
    }

    // Scrollbar
    let total_h = lines.len() as f32 * line_h + 8.0;
    let scrollbar_visible_h = rect.height();
    let max_scroll = (total_h - scrollbar_visible_h).max(0.0);
    if max_scroll > 0.0 {
        let thumb_h = (scrollbar_visible_h * scrollbar_visible_h / total_h).max(20.0);
        let thumb_y = rect.top + (scroll_y / max_scroll) * (scrollbar_visible_h - thumb_h);
        paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(rect.right - 6.0, thumb_y, 4.0, thumb_h),
                2.0,
                2.0,
            ),
            &paint,
        );
    }

    canvas.restore();
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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
fn draw_dep_dropdown(
    canvas: &Canvas,
    dd: Rect,
    dep: &DependencyEdit,
    hovered_row: Option<usize>,
    scroll: usize,
    edit_milestone_id: Option<MilestoneId>,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    // Background
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

    // Filter input
    let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), DEP_DROPDOWN_FILTER_H);
    draw_text_input(canvas, filter_rect, &dep.dep_filter, true, false, cache);

    // Divider
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + DEP_DROPDOWN_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    let filter = dep.dep_filter.content.to_lowercase();
    let mut items: Vec<(NodeId, String)> = Vec::new();

    // Plan Start
    if filter.is_empty() || "plan start".contains(filter.as_str()) {
        items.push((NodeId::PlanStart, "Plan Start".to_string()));
    }

    // Tasks (all tasks are valid deps for milestones)
    let mut task_items: Vec<(NodeId, String)> = plan
        .tasks
        .iter()
        .filter(|(_, t)| filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()))
        .map(|(id, t)| (NodeId::Task(*id), t.name.clone()))
        .collect();
    task_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(task_items);

    // Milestones (exclude the one being edited)
    let mut ms_items: Vec<(NodeId, String)> = plan
        .milestones
        .iter()
        .filter(|(id, m)| {
            (edit_milestone_id != Some(**id))
                && (filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()))
        })
        .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone()))
        .collect();
    ms_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(ms_items);

    let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        dd.height() - DEP_DROPDOWN_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if items.is_empty() {
        let msg = "No matches";
        if let Some(blob) = TextBlob::new(msg, &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (dd.left + 8.0, list_top + 8.0 - sm.ascent), &paint);
        }
    } else {
        let end = (scroll + MAX_DEP_DROPDOWN_ROWS).min(items.len());
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        for (vis, (node_id, name)) in items[scroll..end].iter().enumerate() {
            let abs = scroll + vis;
            let ry = list_top + vis as f32 * DEP_DROPDOWN_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), DEP_DROPDOWN_ROW_H);

            if hovered_row == Some(abs) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            // Tick if already selected
            if dep.target == Some(*node_id) {
                let tx = dd.left + 10.0;
                let ty = ry + DEP_DROPDOWN_ROW_H / 2.0;
                paint.set_color(Color::from(BTN_PRIMARY_BG));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.5);
                let mut pb = PathBuilder::new();
                pb.move_to((tx, ty));
                pb.line_to((tx + 3.0, ty + 3.0));
                pb.line_to((tx + 7.0, ty - 3.0));
                canvas.draw_path(&pb.detach(), &paint);
                paint.set_style(PaintStyle::Fill);
            }

            if let Some(blob) = TextBlob::new(name, &cache.small_font) {
                let ty = ry + (DEP_DROPDOWN_ROW_H - sm_h) / 2.0 - sm.ascent;
                let fg = if *node_id == NodeId::PlanStart {
                    DEP_PLAN_START_FG
                } else {
                    ITEM_FG
                };
                paint.set_color(Color::from(fg));
                canvas.draw_text_blob(&blob, (dd.left + 22.0, ty), &paint);
            }
        }
    }

    canvas.restore();
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

        crate::ui::window_chrome::draw_chevron_btn(canvas, back_btn, self.hovered_back);

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
        draw_multi_line_input(
            canvas,
            Self::full_input_rect(ROW_DESC, width, height),
            &self.description,
            self.focused == TextField::Description,
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

        // Dependencies section
        let dep_lbl_y = Self::dep_label_y(width, height);
        let dep_label_text = if self.dep_error {
            "Dependencies (at least one required)"
        } else {
            "Dependencies"
        };
        if let Some(blob) = TextBlob::new(dep_label_text, &cache.small_font) {
            paint.set_color(Color::from(if self.dep_error {
                BTN_DANGER_BG
            } else {
                LABEL_FG
            }));
            canvas.draw_text_blob(&blob, (lx, dep_lbl_y + label_y_offset), &paint);
        }

        let dep_list = Self::dep_list_rect(width, height);

        paint.set_color(Color::from(if self.dep_error {
            BTN_DANGER_BG
        } else {
            INPUT_BORDER
        }));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(if self.dep_error { 2.0 } else { 1.0 });
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_list, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);

        canvas.save();
        canvas.clip_rect(dep_list, ClipOp::Intersect, false);
        canvas.translate((0.0, -self.dep_scroll_y));

        if self.dependencies.is_empty() {
            if let Some(blob) = TextBlob::new("No dependencies added yet", &cache.small_font) {
                let (_, sm2) = cache.small_font.metrics();
                let ty = dep_list.top + (DEP_ROW_H - (sm2.descent - sm2.ascent)) / 2.0 - sm2.ascent;
                paint.set_color(Color::from(MUTED_FG));
                canvas.draw_text_blob(&blob, (dep_list.left + 12.0, ty), &paint);
            }
        } else {
            for (abs, dep) in self.dependencies.iter().enumerate() {
                if abs > 0 {
                    paint.set_color(Color::from(DIVIDER_COLOR));
                    canvas.draw_rect(
                        Rect::from_xywh(
                            dep_list.left,
                            dep_list.top + abs as f32 * DEP_ROW_H,
                            dep_list.width(),
                            1.0,
                        ),
                        &paint,
                    );
                }

                let target_rect = Self::dep_target_rect(dep_list, abs);
                let lag_rect = Self::dep_lag_rect(dep_list, abs);
                let rm_rect = Self::dep_remove_rect(dep_list, abs);
                let dd_open = self.dep_dropdown_open_for == Some(abs);

                let rrect = RRect::new_rect_xy(target_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
                paint.set_color(Color::from(INPUT_BG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rrect(rrect, &paint);
                paint.set_color(if dd_open {
                    Color::from(INPUT_BORDER_FOCUS)
                } else if dep.hovered_target {
                    Color::from(MUTED_FG)
                } else {
                    Color::from(INPUT_BORDER)
                });
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.0);
                canvas.draw_rrect(rrect, &paint);
                paint.set_style(PaintStyle::Fill);

                let target_name: String = match dep.target {
                    Some(NodeId::PlanStart) => "Plan Start".to_string(),
                    Some(NodeId::Task(id)) => plan
                        .tasks
                        .get(&id)
                        .map(|t| t.name.clone())
                        .unwrap_or_default(),
                    Some(NodeId::Milestone(id)) => plan
                        .milestones
                        .get(&id)
                        .map(|m| m.name.clone())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                let (target_text, target_color) = if dep.target.is_none() {
                    ("Select dependency…".to_string(), Color::from(MUTED_FG))
                } else if dep.target == Some(NodeId::PlanStart) {
                    (target_name, Color::from(DEP_PLAN_START_FG))
                } else {
                    (target_name, Color::from(INPUT_FG))
                };

                canvas.save();
                canvas.clip_rect(
                    Rect::from_xywh(
                        target_rect.left + 6.0,
                        target_rect.top,
                        target_rect.width() - 22.0,
                        target_rect.height(),
                    ),
                    ClipOp::Intersect,
                    false,
                );
                if let Some(blob) = TextBlob::new(&target_text, &cache.small_font) {
                    let (_, sm) = cache.small_font.metrics();
                    let ty = target_rect.top
                        + (target_rect.height() - (sm.descent - sm.ascent)) / 2.0
                        - sm.ascent;
                    paint.set_color(target_color);
                    canvas.draw_text_blob(&blob, (target_rect.left + 6.0, ty), &paint);
                }
                canvas.restore();

                // Chevron on target button
                {
                    let cx = target_rect.right - 12.0;
                    let cy = target_rect.top + target_rect.height() / 2.0;
                    let s = 3.5;
                    let mut pb = PathBuilder::new();
                    if dd_open {
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
                }

                let lag_focused = self.focused_dep_lag == Some(abs);
                draw_text_input(canvas, lag_rect, &dep.lag_input, lag_focused, false, cache);
                if dep.lag_input.content.is_empty()
                    && !lag_focused
                    && let Some(blob) = TextBlob::new("0", &cache.small_font)
                {
                    let (_, sm) = cache.small_font.metrics();
                    let ty = lag_rect.top + (lag_rect.height() - (sm.descent - sm.ascent)) / 2.0
                        - sm.ascent;
                    paint.set_color(Color::from(GHOST_FG));
                    canvas.draw_text_blob(&blob, (lag_rect.left + 8.0, ty), &paint);
                }

                if dep.hovered_remove {
                    let r = rm_rect.width().min(rm_rect.height()) / 2.0 - 2.0;
                    let cx = rm_rect.left + rm_rect.width() / 2.0;
                    let cy = rm_rect.top + rm_rect.height() / 2.0;
                    paint.set_color(Color::from(ERROR_BG));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_circle((cx, cy), r, &paint);
                    paint.set_style(PaintStyle::Fill);
                }
                {
                    let cx = rm_rect.left + rm_rect.width() / 2.0;
                    let cy = rm_rect.top + rm_rect.height() / 2.0;
                    let s = 5.0;
                    let mut pb = PathBuilder::new();
                    pb.move_to((cx - s, cy - s));
                    pb.line_to((cx + s, cy + s));
                    pb.move_to((cx + s, cy - s));
                    pb.line_to((cx - s, cy + s));
                    paint.set_color(if dep.hovered_remove {
                        Color::from(ICON_DELETE_COLOR)
                    } else {
                        Color::from(OVERLAY_DARK)
                    });
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&pb.detach(), &paint);
                    paint.set_style(PaintStyle::Fill);
                }
            }
        }

        canvas.restore(); // end dep list clip

        // Dep list scrollbar
        let total_dep_h = self.dependencies.len() as f32 * DEP_ROW_H;
        let visible_dep_h = dep_list.height();
        let max_dep_scroll = (total_dep_h - visible_dep_h).max(0.0);
        if max_dep_scroll > 0.0 {
            let thumb_h = (visible_dep_h * visible_dep_h / total_dep_h).max(20.0);
            let thumb_y =
                dep_list.top + (self.dep_scroll_y / max_dep_scroll) * (visible_dep_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        dep_list.right - SCROLLBAR_W - 2.0,
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

        // Dep plus button
        let dep_plus_rect = Self::dep_plus_rect(width, height);
        paint.set_color(Color::from(if self.hovered_dep_plus {
            0xff_e0e0e0_u32
        } else {
            SUBTLE_BG
        }));
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);
        {
            let cx = dep_plus_rect.left + dep_plus_rect.width() / 2.0;
            let cy = dep_plus_rect.top + dep_plus_rect.height() / 2.0;
            let s = 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to((cx - s, cy));
            pb.line_to((cx + s, cy));
            pb.move_to((cx, cy - s));
            pb.line_to((cx, cy + s));
            paint.set_color(Color::from(0xff_555555_u32));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
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

        canvas.restore(); // end content scroll region

        // Scheduler error: red border + fixed banner below title bar.
        if let Some(ref err_msg) = self.scheduler_error {
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(2.5);
            canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);
            paint.set_style(PaintStyle::Fill);

            const PAD_V: f32 = 8.0;
            let inset = 2.0;
            let banner_x = panel.left + inset;
            let banner_w = panel.width() - 2.0 * inset;
            let max_w = banner_w - 2.0 * PLAN_FORM_PADDING;
            let (_, bm) = cache.small_font.metrics();
            let line_h = (bm.descent - bm.ascent).ceil() + 2.0;
            let lines = wrap_text(err_msg, &cache.small_font, max_w);
            let banner_h = PAD_V * 2.0 + line_h * lines.len() as f32;
            let banner_rect =
                Rect::from_xywh(banner_x, panel.top + TITLE_H + 1.0, banner_w, banner_h);
            paint.set_color(Color::from(ERROR_BG));
            canvas.draw_rect(banner_rect, &paint);
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            let text_x = banner_x + PLAN_FORM_PADDING;
            let mut text_y = banner_rect.top + PAD_V - bm.ascent;
            for line in &lines {
                if let Some(blob) = TextBlob::new(line.as_str(), &cache.small_font) {
                    canvas.draw_text_blob(&blob, (text_x, text_y), &paint);
                }
                text_y += line_h;
            }
        }

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

        // Dep dropdown (on top of everything, not clipped by scroll)
        if let Some(dep_idx) = self.dep_dropdown_open_for
            && dep_idx < self.dependencies.len()
        {
            let dep_list2 = Self::dep_list_rect(width, height);
            let adjusted_dep_list = Rect::from_xywh(
                dep_list2.left,
                dep_list2.top - scroll_y - self.dep_scroll_y,
                dep_list2.width(),
                dep_list2.height(),
            );
            let dd_rect = Self::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
            draw_dep_dropdown(
                canvas,
                dd_rect,
                &self.dependencies[dep_idx],
                self.dep_dropdown_hovered,
                self.dep_dropdown_scroll,
                self.mode_milestone_id(),
                plan,
                cache,
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
        plan: &Plan,
    ) -> FloatingWindowOutcome {
        let pt = Point::new(x, y);
        let scroll_y = self.effective_scroll(width, height);
        let pt_form = Point::new(x, y + scroll_y);
        let panel = Self::panel_rect(width, height);
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
        } else if let Some(dep_idx) = self.dep_dropdown_open_for
            && dep_idx < self.dependencies.len()
        {
            let dep_list2 = Self::dep_list_rect(width, height);
            let adjusted_dep_list = Rect::from_xywh(
                dep_list2.left,
                dep_list2.top - scroll_y - self.dep_scroll_y,
                dep_list2.width(),
                dep_list2.height(),
            );
            let dd = Self::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
            // compute filtered count
            let dep_list_filtered = {
                let dep_ref = &self.dependencies[dep_idx];
                let filter = dep_ref.dep_filter.content.to_lowercase();
                let mut count = 0usize;
                if filter.is_empty() || "plan start".contains(filter.as_str()) {
                    count += 1;
                }
                for t in plan.tasks.values() {
                    if filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                let edit_ms_id = self.mode_milestone_id();
                for (id, m) in &plan.milestones {
                    if edit_ms_id == Some(*id) {
                        continue;
                    }
                    if filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                count
            };
            let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
            let new_hov = if y >= list_top && x >= dd.left && x <= dd.right {
                let abs = ((y - list_top) / DEP_DROPDOWN_ROW_H) as usize + self.dep_dropdown_scroll;
                if abs < dep_list_filtered {
                    Some(abs)
                } else {
                    None
                }
            } else {
                None
            };
            set!(self.dep_dropdown_hovered, new_hov);
        } else {
            let new_back = Self::back_btn_rect(width, height).contains(pt);
            let new_save = Self::save_btn_rect(width, height).contains(pt_form);
            set!(self.hovered_back, new_back);
            set!(self.hovered_save, new_save);

            set!(
                self.cursor_in_desc,
                Self::full_input_rect(ROW_DESC, width, height).contains(pt_form)
            );

            let new_ck = Self::constraint_kind_btn_rects(width, height)
                .iter()
                .position(|r| r.contains(pt_form));
            set!(self.hovered_constraint_kind, new_ck);

            let new_ct = self.constraint_kind != ConstraintSel::None
                && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form);
            set!(self.constraint_date.hovered_trigger, new_ct);

            // Dep dropdown hover
            let dep_list2 = Self::dep_list_rect(width, height);
            let in_dep_list = {
                let dep_list_screen = Rect::from_xywh(
                    dep_list2.left,
                    dep_list2.top - scroll_y,
                    dep_list2.width(),
                    dep_list2.height(),
                );
                dep_list_screen.contains(Point::new(x, y))
            };
            set!(self.cursor_in_dep_list, in_dep_list);
            set!(self.hovered_dep_plus, {
                let dep_plus = Self::dep_plus_rect(width, height);
                let dep_plus_screen = Rect::from_xywh(
                    dep_plus.left,
                    dep_plus.top - scroll_y,
                    dep_plus.width(),
                    dep_plus.height(),
                );
                dep_plus_screen.contains(Point::new(x, y))
            });
            let pt_dep = Point::new(x, y + scroll_y + self.dep_scroll_y);
            for dep in &mut self.dependencies {
                dep.hovered_target = false;
                dep.hovered_remove = false;
            }
            for (i, dep) in self.dependencies.iter_mut().enumerate() {
                let target_rect = Self::dep_target_rect(dep_list2, i);
                let remove_rect = Self::dep_remove_rect(dep_list2, i);
                if target_rect.contains(pt_dep) {
                    dep.hovered_target = true;
                    changed = true;
                }
                if remove_rect.contains(pt_dep) {
                    dep.hovered_remove = true;
                    changed = true;
                }
            }
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
        modifiers: &Modifiers,
        plan: &Plan,
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
                    let today = chrono::Local::now().date_naive();
                    let on_today_month = self.constraint_date.nav_year == today.year()
                        && self.constraint_date.nav_month == today.month();
                    if on_today_month {
                        self.constraint_date.value = Some(today);
                        self.constraint_date_error = false;
                        self.close_calendar();
                    } else {
                        self.constraint_date.nav_year = today.year();
                        self.constraint_date.nav_month = today.month();
                    }
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

        // Dep dropdown
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            if dep_idx < self.dependencies.len() {
                let dep_list2 = Self::dep_list_rect(width, height);
                let adjusted_dep_list = Rect::from_xywh(
                    dep_list2.left,
                    dep_list2.top - scroll_y - self.dep_scroll_y,
                    dep_list2.width(),
                    dep_list2.height(),
                );
                let panel = Self::panel_rect(width, height);
                let dd = Self::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
                if dd.contains(pt) {
                    let filter_rect =
                        Rect::from_xywh(dd.left, dd.top, dd.width(), DEP_DROPDOWN_FILTER_H);
                    if filter_rect.contains(pt) {
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
                    if y >= list_top {
                        let abs = ((y - list_top) / DEP_DROPDOWN_ROW_H) as usize
                            + self.dep_dropdown_scroll;
                        let filter = self.dependencies[dep_idx].dep_filter.content.to_lowercase();
                        let mut items: Vec<(NodeId, String)> = Vec::new();
                        if filter.is_empty() || "plan start".contains(filter.as_str()) {
                            items.push((NodeId::PlanStart, "Plan Start".to_string()));
                        }
                        let edit_milestone_id = self.mode_milestone_id();
                        let mut task_items: Vec<(NodeId, String)> = plan
                            .tasks
                            .iter()
                            .filter(|(_, t)| {
                                filter.is_empty() || t.name.to_lowercase().contains(filter.as_str())
                            })
                            .map(|(id, t)| (NodeId::Task(*id), t.name.clone()))
                            .collect();
                        task_items.sort_by(|a, b| a.1.cmp(&b.1));
                        items.extend(task_items);
                        let mut ms_items: Vec<(NodeId, String)> = plan
                            .milestones
                            .iter()
                            .filter(|(id, m)| {
                                edit_milestone_id != Some(**id)
                                    && (filter.is_empty()
                                        || m.name.to_lowercase().contains(filter.as_str()))
                            })
                            .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone()))
                            .collect();
                        ms_items.sort_by(|a, b| a.1.cmp(&b.1));
                        items.extend(ms_items);
                        if let Some((node_id, _)) = items.get(abs) {
                            self.dependencies[dep_idx].target = Some(*node_id);
                            self.dep_error = false;
                        }
                        self.close_dep_dropdown();
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            self.close_dep_dropdown();
            if !Self::panel_rect(width, height).contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(pt_form) {
            return self.try_submit(plan, sender);
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
        // Description (multi-line) handled separately
        let desc_rect = Self::full_input_rect(ROW_DESC, width, height);
        if desc_rect.contains(pt_form) {
            self.set_focus(TextField::Description);
            let x_in_box = x - desc_rect.left;
            let y_in_box = pt_form.y - desc_rect.top;
            let inner_width = desc_rect.width() - 16.0;
            self.description.cursor = self.description.cursor_for_click(
                x_in_box,
                y_in_box,
                inner_width,
                &cache.font,
                DESC_LINE_H,
            );
            // Ctrl+click: open link under cursor
            if modifiers.state().control_key() {
                let cursor = self.description.cursor;
                let content = self.description.content.clone();
                let links = MultiLineInput::find_links(&content);
                if let Some(range) = links.iter().find(|r| r.contains(&cursor)) {
                    MultiLineInput::open_url(&content[range.clone()]);
                }
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if Self::full_input_rect(ROW_NAME, width, height).contains(pt_form) {
            self.set_focus(TextField::Name);
            let rect = Self::full_input_rect(ROW_NAME, width, height);
            let x_in_inner = x - (rect.left + 8.0) + self.name.scroll_x.get();
            self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Dep list interactions
        let dep_list2 = Self::dep_list_rect(width, height);
        let dep_plus = Self::dep_plus_rect(width, height);

        if dep_plus.contains(pt_form) {
            self.dependencies.push(DependencyEdit::new());
            let total_h = self.dependencies.len() as f32 * DEP_ROW_H;
            let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
            self.dep_scroll_y = (total_h - visible_h).max(0.0);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if dep_list2.contains(pt_form) {
            let pt_dep = Point::new(x, y + scroll_y + self.dep_scroll_y);
            for abs in 0..self.dependencies.len() {
                if Self::dep_remove_rect(dep_list2, abs).contains(pt_dep) {
                    self.dependencies.remove(abs);
                    self.clamp_dep_scroll_y();
                    if let Some(ref mut fl) = self.focused_dep_lag {
                        if *fl == abs {
                            self.focused_dep_lag = None;
                        } else if *fl > abs {
                            *fl -= 1;
                        }
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if Self::dep_target_rect(dep_list2, abs).contains(pt_dep) {
                    self.open_dep_dropdown(abs);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if Self::dep_lag_rect(dep_list2, abs).contains(pt_dep) {
                    self.focused_dep_lag = Some(abs);
                    self.name.focused = false;
                    self.description.focused = false;
                    let lag_rect = Self::dep_lag_rect(dep_list2, abs);
                    let x_in_inner =
                        x - (lag_rect.left + 8.0) + self.dependencies[abs].lag_input.scroll_x.get();
                    self.dependencies[abs].lag_input.cursor = self.dependencies[abs]
                        .lag_input
                        .cursor_for_x(x_in_inner, &cache.font);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if !Self::panel_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        sender: &PlanRequestSender,
        width: f32,
        height: f32,
        plan: &Plan,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if self.calendar_open {
            if *key == Key::Named(NamedKey::Escape) {
                self.close_calendar();
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }

        // Dep dropdown open: route keys to filter input
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            match key {
                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                    self.close_dep_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Backspace) => {
                    if dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.backspace();
                        self.dep_dropdown_scroll = 0;
                        self.dep_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    if dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.insert_str(" ");
                        self.dep_dropdown_scroll = 0;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Character(c) => {
                    if c.chars().all(|ch| !ch.is_control()) && dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.insert_str(c.as_str());
                        self.dep_dropdown_scroll = 0;
                        self.dep_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Dep lag input focused
        if let Some(lag_idx) = self.focused_dep_lag
            && lag_idx < self.dependencies.len()
        {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.focused_dep_lag = None;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Tab) => {
                    self.focused_dep_lag = None;
                    self.set_focus(TextField::Name);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => return self.try_submit(plan, sender),
                Key::Named(NamedKey::Backspace) => {
                    self.dependencies[lag_idx].lag_input.backspace();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.dependencies[lag_idx].lag_input.move_left();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.dependencies[lag_idx].lag_input.move_right();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Home) => {
                    self.dependencies[lag_idx].lag_input.move_home();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::End) => {
                    self.dependencies[lag_idx].lag_input.move_end();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    return FloatingWindowOutcome::default();
                }
                Key::Character(c) => {
                    if c.chars()
                        .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-')
                    {
                        self.dependencies[lag_idx].lag_input.insert_str(c.as_str());
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    return FloatingWindowOutcome::default();
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Description (multi-line): Enter inserts newline, not submit
        if self.focused == TextField::Description {
            let desc_rect = Self::full_input_rect(ROW_DESC, width, height);
            let inner_width = desc_rect.width() - 16.0;
            let (_, metrics) = cache.font.metrics();
            let line_h = metrics.descent - metrics.ascent + 2.0;
            let visible_h = DESC_H - 8.0;
            match key {
                Key::Named(NamedKey::Escape) => return FloatingWindowOutcome::close(),
                Key::Named(NamedKey::Enter) => {
                    self.description.insert_newline();
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Tab) => {
                    self.set_focus(TextField::Name);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Backspace) => {
                    self.description.backspace();
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.description.move_left();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.description.move_right();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.description.move_up(inner_width, &cache.font);
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.description.move_down(inner_width, &cache.font);
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Home) => {
                    self.description.move_to_start();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::End) => {
                    self.description.move_to_end();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    self.description.insert_char(' ');
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Character(c) => {
                    if c.chars().all(|ch| !ch.is_control()) {
                        for ch in c.chars() {
                            self.description.insert_char(ch);
                        }
                        self.description.x_hint = None;
                        self.description.scroll_to_cursor(
                            inner_width,
                            &cache.font,
                            line_h,
                            visible_h,
                        );
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    return FloatingWindowOutcome::default();
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => self.try_submit(plan, sender),
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
        for dep in &mut self.dependencies {
            dep.hovered_target = false;
            dep.hovered_remove = false;
        }
        self.hovered_dep_plus = false;
        self.dep_dropdown_hovered = None;
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        // Scroll dep dropdown if open
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            if dep_idx < self.dependencies.len() {
                let filter = self.dependencies[dep_idx].dep_filter.content.to_lowercase();
                let mut count = 0usize;
                if filter.is_empty() || "plan start".contains(filter.as_str()) {
                    count += 1;
                }
                for t in plan.tasks.values() {
                    if filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                for (id, m) in &plan.milestones {
                    if self.mode_milestone_id() == Some(*id) {
                        continue;
                    }
                    if filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                let max = count.saturating_sub(MAX_DEP_DROPDOWN_ROWS);
                if max == 0 {
                    return FloatingWindowOutcome::default();
                }
                let new_scroll = if delta_y > 0.0 {
                    self.dep_dropdown_scroll.saturating_sub(1)
                } else {
                    (self.dep_dropdown_scroll + 1).min(max)
                };
                if new_scroll != self.dep_dropdown_scroll {
                    self.dep_dropdown_scroll = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::default();
        }

        // Scroll dep list independently when cursor is inside it
        if self.cursor_in_dep_list {
            let content_h = self.dependencies.len() as f32 * DEP_ROW_H;
            let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
            let max_dscroll = (content_h - visible_h).max(0.0);
            if max_dscroll > 0.0 {
                let new_scroll = (self.dep_scroll_y - delta_y * 40.0).clamp(0.0, max_dscroll);
                if (new_scroll - self.dep_scroll_y).abs() > f32::EPSILON {
                    self.dep_scroll_y = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                return FloatingWindowOutcome::default();
            }
        }

        // Scroll description box independently when cursor is inside it
        if self.cursor_in_desc {
            let line_count = self.description.content.split('\n').count().max(1);
            let total_h = line_count as f32 * DESC_LINE_H + 8.0;
            let visible_h = DESC_H;
            let max_dscroll = (total_h - visible_h).max(0.0);
            let cur = self.description.scroll_y.get();
            let new_scroll = (cur - delta_y * 40.0).clamp(0.0, max_dscroll.max(cur));
            if (new_scroll - cur).abs() > f32::EPSILON {
                self.description.scroll_y.set(new_scroll);
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            return FloatingWindowOutcome::default();
        }

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
